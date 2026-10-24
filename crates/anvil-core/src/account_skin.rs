//! Public skin metadata, independent of account credentials and authentication.
use crate::{Context, Error, Result, account::AccountProfile, catalog_cache};
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::{Client, redirect::Policy};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountSkin {
  pub url: String,
  pub model: String,
}

fn texture_url(value: &str) -> Result<String> {
  let mut url = url::Url::parse(value).map_err(|_| invalid())?;
  let hash = url.path().strip_prefix("/texture/").ok_or_else(invalid)?;
  if !matches!(url.scheme(), "http" | "https")
    || url.host_str() != Some("textures.minecraft.net")
    || !url.username().is_empty()
    || url.password().is_some()
    || url.port().is_some()
    || url.query().is_some()
    || url.fragment().is_some()
    || hash.len() != 64
    || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
  {
    return Err(invalid());
  }
  url.set_scheme("https").map_err(|_| invalid())?;
  Ok(url.into())
}

fn invalid() -> Error {
  Error::Metadata("Minecraft skin metadata is invalid".into())
}

fn parse(bytes: &[u8], id: &str) -> Result<Option<AccountSkin>> {
  let profile: serde_json::Value = serde_json::from_slice(bytes)?;
  if profile["id"].as_str() != Some(id) {
    return Err(invalid());
  }
  let Some(properties) = profile["properties"].as_array() else {
    return Ok(None);
  };
  let Some(property) = properties.iter().find(|entry| entry["name"] == "textures") else {
    return Ok(None);
  };
  let value = STANDARD
    .decode(property["value"].as_str().ok_or_else(invalid)?)
    .map_err(|_| invalid())?;
  let textures: serde_json::Value = serde_json::from_slice(&value)?;
  let skin = &textures["textures"]["SKIN"];
  if skin.is_null() {
    return Ok(None);
  }
  let model = skin["metadata"]["model"].as_str().unwrap_or("classic");
  if !matches!(model, "classic" | "slim") {
    return Err(invalid());
  }
  Ok(Some(AccountSkin {
    url: texture_url(skin["url"].as_str().ok_or_else(invalid)?)?,
    model: model.to_owned(),
  }))
}

/// Read by UUID so existing saved accounts do not need to authorize again.
/// No bearer tokens are sent to the public session server or texture host.
pub async fn read(context: &Context, id: &str) -> Result<Option<AccountSkin>> {
  AccountProfile::new(id, "Player", None)?;
  let id = id.replace('-', "").to_ascii_lowercase();
  let cancel = crate::CancellationToken::new();
  let _guard = context
    .cache_lock(format!("account-skin:{id}"), &cancel)
    .await?;
  let cached =
    catalog_cache::load::<Option<AccountSkin>>(context, catalog_cache::ACCOUNT_SKINS, &id)
      .await
      .ok()
      .flatten();
  let cached = cached.filter(|snapshot| {
    snapshot.value.as_ref().is_none_or(|skin| {
      texture_url(&skin.url).is_ok() && matches!(skin.model.as_str(), "classic" | "slim")
    })
  });
  if let Some(snapshot) = &cached
    && snapshot
      .fetched_at
      .elapsed()
      .is_ok_and(|age| age < Duration::from_secs(3600))
  {
    return Ok(snapshot.value.clone());
  }
  let result = fetch(&id).await;
  match result {
    Ok(skin) => {
      // Public metadata only; persistence failure must not prevent the avatar.
      let _ = catalog_cache::save(
        context,
        catalog_cache::ACCOUNT_SKINS,
        &id,
        skin.clone(),
        SystemTime::now(),
      )
      .await;
      Ok(skin)
    }
    Err(error) => cached.map(|snapshot| snapshot.value).ok_or(error),
  }
}

async fn fetch(id: &str) -> Result<Option<AccountSkin>> {
  let client = Client::builder()
    .timeout(Duration::from_secs(8))
    .redirect(Policy::none())
    .build()
    .map_err(|_| invalid())?;
  let mut response = client
    .get(format!(
      "https://sessionserver.mojang.com/session/minecraft/profile/{id}"
    ))
    .send()
    .await
    .map_err(|_| Error::Metadata("Minecraft skin service is unavailable".into()))?
    .error_for_status()
    .map_err(|_| Error::Metadata("Minecraft skin service rejected the request".into()))?;
  if response.status().as_u16() == 204 {
    return Ok(None);
  }
  if response
    .content_length()
    .is_some_and(|size| size > 64 * 1024)
  {
    return Err(invalid());
  }
  let mut bytes = Vec::new();
  while let Some(chunk) = response.chunk().await.map_err(|_| invalid())? {
    if bytes.len() + chunk.len() > 64 * 1024 {
      return Err(invalid());
    }
    bytes.extend_from_slice(&chunk);
  }
  parse(&bytes, id)
}

#[cfg(test)]
#[path = "../tests/account_skin.rs"]
mod tests;
