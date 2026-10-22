//! Microsoft device-code authorization and native credential-vault storage.
use std::{time::Duration, time::SystemTime};

use keyring::{Entry, Error as KeyringError};
use reqwest::{Client, StatusCode, header::CONTENT_TYPE};
use serde::Deserialize;
use tokio::time::{Instant, sleep};
use tokio_util::sync::CancellationToken;
use url::{Url, form_urlencoded};

use super::{
  AccessToken, AccountProfile, AccountProvider, AuthenticatedAccount, AuthorizationEvent,
  AuthorizationFuture, CredentialStore, MicrosoftClientId, RefreshToken,
};
use crate::{Error, Result};

const CLIENT_SCOPE: &str = "XboxLive.signin offline_access";
const AUTH_BODY_LIMIT: usize = 64 * 1024;
const AUTH_TIMEOUT: Duration = Duration::from_secs(20);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_DEVICE_CODE_LIFETIME: u64 = 60 * 60;
const MAX_MINECRAFT_TOKEN_LIFETIME: u64 = 7 * 24 * 60 * 60;
// Stable system-vault namespace in reverse-domain style; changing it strands saved tokens.
const KEYRING_SERVICE: &str = "app.mivinci.anvil.minecraft";

/// Microsoft device-code OAuth, Xbox Live/XSTS, and Minecraft Services provider.
#[derive(Clone)]
pub struct MicrosoftAccountProvider {
  http: Client,
  endpoints: Endpoints,
}

#[derive(Clone)]
struct Endpoints {
  device_code: Url,
  token: Url,
  xbox_user_auth: Url,
  xbox_xsts_auth: Url,
  minecraft_login: Url,
  minecraft_profile: Url,
}

impl Endpoints {
  fn production() -> Result<Self> {
    Ok(Self {
      device_code: Url::parse("https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode")
        .map_err(|_| Error::Authentication("invalid Microsoft device-code endpoint".into()))?,
      token: Url::parse("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .map_err(|_| Error::Authentication("invalid Microsoft token endpoint".into()))?,
      xbox_user_auth: Url::parse("https://user.auth.xboxlive.com/user/authenticate")
        .map_err(|_| Error::Authentication("invalid Xbox user-auth endpoint".into()))?,
      xbox_xsts_auth: Url::parse("https://xsts.auth.xboxlive.com/xsts/authorize")
        .map_err(|_| Error::Authentication("invalid Xbox XSTS endpoint".into()))?,
      minecraft_login: Url::parse(
        "https://api.minecraftservices.com/authentication/login_with_xbox",
      )
      .map_err(|_| Error::Authentication("invalid Minecraft login endpoint".into()))?,
      minecraft_profile: Url::parse("https://api.minecraftservices.com/minecraft/profile")
        .map_err(|_| Error::Authentication("invalid Minecraft profile endpoint".into()))?,
    })
  }

  #[cfg(test)]
  fn loopback(base: &Url) -> Result<Self> {
    let join = |path: &str| {
      base
        .join(path)
        .map_err(|_| Error::Authentication("invalid test authorization endpoint".into()))
    };
    Ok(Self {
      device_code: join("devicecode")?,
      token: join("token")?,
      xbox_user_auth: join("xbox/user")?,
      xbox_xsts_auth: join("xbox/xsts")?,
      minecraft_login: join("minecraft/login")?,
      minecraft_profile: join("minecraft/profile")?,
    })
  }
}

impl MicrosoftAccountProvider {
  /// Creates a provider for Microsoft's production authorization endpoints.
  /// The caller supplies Anvil's registered public OAuth client ID to `AccountService`.
  pub fn new() -> Result<Self> {
    Self::with_endpoints(Endpoints::production())
  }

  fn with_endpoints(endpoints: Result<Endpoints>) -> Result<Self> {
    let builder = Client::builder()
      .connect_timeout(CONNECT_TIMEOUT)
      .timeout(AUTH_TIMEOUT)
      .pool_max_idle_per_host(1)
      .redirect(reqwest::redirect::Policy::none());
    #[cfg(test)]
    let builder = builder.no_proxy();
    let http = builder
      .build()
      .map_err(|_| Error::Authentication("cannot initialize authorization HTTP client".into()))?;
    Ok(Self {
      http,
      endpoints: endpoints?,
    })
  }

  #[cfg(test)]
  pub(super) fn for_test(base: &Url) -> Result<Self> {
    Self::with_endpoints(Endpoints::loopback(base))
  }

  async fn authorize_device_code(
    &self,
    client_id: &MicrosoftClientId,
    cancel: &CancellationToken,
    events: &tokio::sync::mpsc::UnboundedSender<AuthorizationEvent>,
  ) -> Result<OAuthTokens> {
    let client_id = client_id.as_str();
    let body = form_body(&[("client_id", client_id), ("scope", CLIENT_SCOPE)]);
    let (status, bytes) = self
      .post_form(&self.endpoints.device_code, body, cancel)
      .await?;
    require_success(status, "Microsoft device-code request")?;
    let device: DeviceCodeResponse = parse_json(&bytes, "Microsoft device-code response")?;
    validate_device_code(&device)?;
    events
      .send(AuthorizationEvent::DeviceCode {
        verification_uri: device.verification_uri.clone(),
        user_code: device.user_code.clone(),
        expires_in_seconds: device.expires_in,
        poll_interval_seconds: device.interval,
      })
      .map_err(|_| Error::Cancelled)?;

    let deadline = Instant::now() + Duration::from_secs(device.expires_in);
    let mut interval = Duration::from_secs(device.interval);
    loop {
      let now = Instant::now();
      if now >= deadline {
        return Err(Error::Authentication(
          "Microsoft device code expired".into(),
        ));
      }
      tokio::select! {
        _ = cancel.cancelled() => return Err(Error::Cancelled),
        _ = sleep(interval.min(deadline - now)) => {}
      }
      if cancel.is_cancelled() {
        return Err(Error::Cancelled);
      }
      let body = form_body(&[
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ("client_id", client_id),
        ("device_code", device.device_code.as_str()),
      ]);
      match self.request_tokens(body, cancel).await? {
        TokenPoll::Authorized(tokens) => return Ok(tokens),
        TokenPoll::Pending => {
          events
            .send(AuthorizationEvent::WaitingForApproval)
            .map_err(|_| Error::Cancelled)?;
        }
        TokenPoll::SlowDown => {
          interval = interval.saturating_add(Duration::from_secs(5));
          events
            .send(AuthorizationEvent::WaitingForApproval)
            .map_err(|_| Error::Cancelled)?;
        }
      }
    }
  }

  async fn request_tokens(&self, body: String, cancel: &CancellationToken) -> Result<TokenPoll> {
    let (status, bytes) = self.post_form(&self.endpoints.token, body, cancel).await?;
    if status.is_success() {
      let tokens: OAuthTokens = parse_json(&bytes, "Microsoft token response")?;
      validate_token(&tokens.access_token, "Microsoft access token")?;
      if tokens.expires_in == 0 || tokens.expires_in > 24 * 60 * 60 {
        return Err(Error::Authentication(
          "Microsoft returned an invalid token lifetime".into(),
        ));
      }
      return Ok(TokenPoll::Authorized(tokens));
    }
    let error: OAuthError = parse_json(&bytes, "Microsoft token error")?;
    match error.error.as_str() {
      "authorization_pending" => Ok(TokenPoll::Pending),
      "slow_down" => Ok(TokenPoll::SlowDown),
      "authorization_declined" => Err(Error::Authentication(
        "Microsoft device-code authorization was declined".into(),
      )),
      "expired_token" | "bad_verification_code" => Err(Error::Authentication(
        "Microsoft device-code authorization expired".into(),
      )),
      _ => Err(Error::Authentication(
        "Microsoft token exchange was rejected".into(),
      )),
    }
  }

  async fn refresh_microsoft_token(
    &self,
    client_id: &MicrosoftClientId,
    refresh_token: &str,
    cancel: &CancellationToken,
  ) -> Result<OAuthTokens> {
    let body = form_body(&[
      ("grant_type", "refresh_token"),
      ("client_id", client_id.as_str()),
      ("refresh_token", refresh_token),
      ("scope", CLIENT_SCOPE),
    ]);
    match self.request_tokens(body, cancel).await? {
      TokenPoll::Authorized(tokens) => Ok(tokens),
      TokenPoll::Pending | TokenPoll::SlowDown => Err(Error::Authentication(
        "Microsoft refresh unexpectedly requires user interaction".into(),
      )),
    }
  }

  async fn finish_login(
    &self,
    microsoft_access_token: &str,
    refresh_token: RefreshToken,
    cancel: &CancellationToken,
  ) -> Result<AuthenticatedAccount> {
    validate_token(microsoft_access_token, "Microsoft access token")?;
    let xbox_user = self
      .post_xbox_json(
        &self.endpoints.xbox_user_auth,
        serde_json::json!({
          "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={microsoft_access_token}")
          },
          "RelyingParty": "http://auth.xboxlive.com",
          "TokenType": "JWT"
        }),
        cancel,
        "Xbox user authentication",
      )
      .await?;

    let xbox_user: XboxAuthResponse = parse_json(&xbox_user, "Xbox user-auth response")?;
    validate_token(&xbox_user.token, "Xbox user token")?;
    let user_hash = xbox_user
      .display_claims
      .xui
      .first()
      .map(|claims| claims.user_hash.as_str())
      .filter(|value| !value.is_empty() && value.len() <= 256)
      .ok_or_else(|| Error::Authentication("Xbox response has no user identity".into()))?;

    let xsts = self
      .post_xbox_json(
        &self.endpoints.xbox_xsts_auth,
        serde_json::json!({
          "Properties": {"SandboxId": "RETAIL", "UserTokens": [xbox_user.token]},
          "RelyingParty": "rp://api.minecraftservices.com/",
          "TokenType": "JWT"
        }),
        cancel,
        "Xbox XSTS authorization",
      )
      .await?;
    let xsts: XboxAuthResponse = parse_json(&xsts, "Xbox XSTS response")?;
    validate_token(&xsts.token, "Xbox XSTS token")?;
    let xsts_user_hash = xsts
      .display_claims
      .xui
      .first()
      .map(|claims| claims.user_hash.as_str())
      .filter(|value| !value.is_empty() && value.len() <= 256)
      .ok_or_else(|| Error::Authentication("Xbox XSTS response has no user identity".into()))?;
    if xsts_user_hash != user_hash {
      return Err(Error::Authentication(
        "Xbox returned inconsistent user identities".into(),
      ));
    }

    let identity_token = format!("XBL3.0 x={user_hash};{}", xsts.token);
    let minecraft_login = self
      .post_json(
        &self.endpoints.minecraft_login,
        serde_json::json!({"identityToken": identity_token}),
        cancel,
        "Minecraft Services login",
      )
      .await?;
    let minecraft_login: MinecraftLoginResponse =
      parse_json(&minecraft_login, "Minecraft login response")?;
    validate_token(&minecraft_login.access_token, "Minecraft access token")?;
    if minecraft_login.expires_in == 0 || minecraft_login.expires_in > MAX_MINECRAFT_TOKEN_LIFETIME
    {
      return Err(Error::Authentication(
        "Minecraft returned an invalid token lifetime".into(),
      ));
    }

    let profile_bytes = self
      .get_bearer(
        &self.endpoints.minecraft_profile,
        &minecraft_login.access_token,
        cancel,
      )
      .await?;
    let profile: MinecraftProfileResponse =
      parse_json(&profile_bytes, "Minecraft profile response")?;
    let profile = AccountProfile::new(profile.id, profile.name, None)?;
    let access_token = AccessToken::new(minecraft_login.access_token)?;
    let expires_at = SystemTime::now()
      .checked_add(Duration::from_secs(minecraft_login.expires_in))
      .ok_or_else(|| Error::Authentication("Minecraft token lifetime overflowed".into()))?;
    AuthenticatedAccount::new(profile, access_token, refresh_token, expires_at, false)
  }

  async fn post_form(
    &self,
    endpoint: &Url,
    body: String,
    cancel: &CancellationToken,
  ) -> Result<(StatusCode, Vec<u8>)> {
    let request = self
      .http
      .post(endpoint.clone())
      .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
      .body(body);
    let response = tokio::select! {
      _ = cancel.cancelled() => return Err(Error::Cancelled),
      result = request.send() => result.map_err(|_| Error::Authentication("authorization request failed".into()))?,
    };
    read_bounded(response, cancel).await
  }

  async fn post_json(
    &self,
    endpoint: &Url,
    body: serde_json::Value,
    cancel: &CancellationToken,
    operation: &str,
  ) -> Result<Vec<u8>> {
    self
      .post_json_request(endpoint, body, cancel, operation, false)
      .await
  }

  async fn post_xbox_json(
    &self,
    endpoint: &Url,
    body: serde_json::Value,
    cancel: &CancellationToken,
    operation: &str,
  ) -> Result<Vec<u8>> {
    self
      .post_json_request(endpoint, body, cancel, operation, true)
      .await
  }

  async fn post_json_request(
    &self,
    endpoint: &Url,
    body: serde_json::Value,
    cancel: &CancellationToken,
    operation: &str,
    xbox_contract: bool,
  ) -> Result<Vec<u8>> {
    let request = self.http.post(endpoint.clone());
    let request = if xbox_contract {
      request.header("x-xbl-contract-version", "1")
    } else {
      request
    }
    .json(&body);
    let response = tokio::select! {
      _ = cancel.cancelled() => return Err(Error::Cancelled),
      result = request.send() => result.map_err(|_| Error::Authentication(format!("{operation} request failed")))?,
    };
    let (status, bytes) = read_bounded(response, cancel).await?;
    require_success(status, operation)?;
    Ok(bytes)
  }

  async fn get_bearer(
    &self,
    endpoint: &Url,
    token: &str,
    cancel: &CancellationToken,
  ) -> Result<Vec<u8>> {
    let request = self.http.get(endpoint.clone()).bearer_auth(token);
    let response = tokio::select! {
      _ = cancel.cancelled() => return Err(Error::Cancelled),
      result = request.send() => result.map_err(|_| Error::Authentication("Minecraft profile request failed".into()))?,
    };
    let (status, bytes) = read_bounded(response, cancel).await?;
    if status == StatusCode::NOT_FOUND {
      return Err(Error::Authentication(
        "Microsoft account has no Minecraft Java profile".into(),
      ));
    }
    require_success(status, "Minecraft profile request")?;
    Ok(bytes)
  }
}

impl AccountProvider for MicrosoftAccountProvider {
  fn authorize(
    &self,
    client_id: MicrosoftClientId,
    cancel: CancellationToken,
    events: tokio::sync::mpsc::UnboundedSender<AuthorizationEvent>,
  ) -> AuthorizationFuture {
    let provider = self.clone();
    Box::pin(async move {
      let tokens = provider
        .authorize_device_code(&client_id, &cancel, &events)
        .await?;
      let refresh_token = tokens
        .refresh_token
        .ok_or_else(|| Error::Authentication("Microsoft did not return a refresh token".into()))?;
      let refresh_token = RefreshToken::new(refresh_token)?;
      provider
        .finish_login(&tokens.access_token, refresh_token, &cancel)
        .await
    })
  }

  fn refresh(
    &self,
    client_id: MicrosoftClientId,
    _account_id: String,
    refresh_token: RefreshToken,
    cancel: CancellationToken,
    _events: tokio::sync::mpsc::UnboundedSender<AuthorizationEvent>,
  ) -> AuthorizationFuture {
    let provider = self.clone();
    Box::pin(async move {
      if cancel.is_cancelled() {
        return Err(Error::Cancelled);
      }
      let previous_refresh_token = refresh_token.expose_secret().to_owned();
      let tokens = provider
        .refresh_microsoft_token(&client_id, &previous_refresh_token, &cancel)
        .await?;
      let refresh_token = tokens.refresh_token.unwrap_or(previous_refresh_token);
      let refresh_token = RefreshToken::new(refresh_token)?;
      provider
        .finish_login(&tokens.access_token, refresh_token, &cancel)
        .await
    })
  }
}

/// Stores Microsoft refresh tokens in the current OS credential vault.
pub struct SystemCredentialStore;

impl CredentialStore for SystemCredentialStore {
  fn load_refresh_token(&self, account_id: &str) -> Result<Option<RefreshToken>> {
    let entry = credential_entry(account_id)?;
    match entry.get_password() {
      Ok(value) => RefreshToken::new(value).map(Some),
      Err(KeyringError::NoEntry) => Ok(None),
      Err(_) => Err(vault_unavailable()),
    }
  }

  fn store_refresh_token(&self, account_id: &str, token: RefreshToken) -> Result<()> {
    credential_entry(account_id)?
      .set_password(token.expose_secret())
      .map_err(|_| vault_unavailable())
  }

  fn delete_refresh_token(&self, account_id: &str) -> Result<()> {
    match credential_entry(account_id)?.delete_credential() {
      Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
      Err(_) => Err(vault_unavailable()),
    }
  }
}

fn credential_entry(account_id: &str) -> Result<Entry> {
  if !super::valid_uuid(account_id) {
    return Err(Error::Invalid("invalid Minecraft account ID".into()));
  }
  Entry::new(KEYRING_SERVICE, &account_id.to_ascii_lowercase()).map_err(|_| vault_unavailable())
}

fn vault_unavailable() -> Error {
  Error::Authentication("system credential vault is unavailable".into())
}

enum TokenPoll {
  Authorized(OAuthTokens),
  Pending,
  SlowDown,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
  device_code: String,
  user_code: String,
  verification_uri: String,
  expires_in: u64,
  #[serde(default = "default_poll_interval")]
  interval: u64,
}

#[derive(Deserialize)]
struct OAuthTokens {
  access_token: String,
  expires_in: u64,
  refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct OAuthError {
  error: String,
}

#[derive(Deserialize)]
struct XboxAuthResponse {
  #[serde(rename = "Token")]
  token: String,
  #[serde(rename = "DisplayClaims")]
  display_claims: XboxDisplayClaims,
}

#[derive(Deserialize)]
struct XboxDisplayClaims {
  xui: Vec<XboxUserClaims>,
}

#[derive(Deserialize)]
struct XboxUserClaims {
  #[serde(rename = "uhs")]
  user_hash: String,
}

#[derive(Deserialize)]
struct MinecraftLoginResponse {
  access_token: String,
  expires_in: u64,
}

#[derive(Deserialize)]
struct MinecraftProfileResponse {
  id: String,
  name: String,
}

fn default_poll_interval() -> u64 {
  5
}

fn form_body(fields: &[(&str, &str)]) -> String {
  let mut serializer = form_urlencoded::Serializer::new(String::new());
  for (key, value) in fields {
    serializer.append_pair(key, value);
  }
  serializer.finish()
}

fn validate_device_code(device: &DeviceCodeResponse) -> Result<()> {
  let verification_uri = Url::parse(&device.verification_uri)
    .map_err(|_| Error::Authentication("Microsoft returned an invalid verification URL".into()))?;
  if verification_uri.scheme() != "https"
    || verification_uri.host_str().is_none()
    || !verification_uri.username().is_empty()
    || verification_uri.password().is_some()
    || device.device_code.is_empty()
    || device.device_code.len() > AUTH_BODY_LIMIT
    || !device
      .device_code
      .bytes()
      .all(|byte| byte.is_ascii_graphic())
    || device.user_code.is_empty()
    || device.user_code.len() > 128
    || !device.user_code.bytes().all(|byte| byte.is_ascii_graphic())
    || !(1..=MAX_DEVICE_CODE_LIFETIME).contains(&device.expires_in)
    || !(1..=60).contains(&device.interval)
  {
    return Err(Error::Authentication(
      "Microsoft returned invalid device-code details".into(),
    ));
  }
  Ok(())
}

fn validate_token(value: &str, label: &str) -> Result<()> {
  if value.is_empty()
    || value.len() > AUTH_BODY_LIMIT
    || !value.bytes().all(|byte| byte.is_ascii_graphic())
  {
    return Err(Error::Authentication(format!("invalid {label}")));
  }
  Ok(())
}

async fn read_bounded(
  mut response: reqwest::Response,
  cancel: &CancellationToken,
) -> Result<(StatusCode, Vec<u8>)> {
  let status = response.status();
  if response
    .content_length()
    .is_some_and(|length| length > AUTH_BODY_LIMIT as u64)
  {
    return Err(Error::Authentication(
      "authorization service response exceeded its size limit".into(),
    ));
  }
  let mut bytes = Vec::with_capacity(response.content_length().unwrap_or(0) as usize);
  loop {
    let chunk = tokio::select! {
      _ = cancel.cancelled() => return Err(Error::Cancelled),
      result = response.chunk() => result.map_err(|_| Error::Authentication("authorization response could not be read".into()))?,
    };
    let Some(chunk) = chunk else { break };
    if bytes.len().saturating_add(chunk.len()) > AUTH_BODY_LIMIT {
      return Err(Error::Authentication(
        "authorization service response exceeded its size limit".into(),
      ));
    }
    bytes.extend_from_slice(&chunk);
  }
  Ok((status, bytes))
}

fn require_success(status: StatusCode, operation: &str) -> Result<()> {
  if status.is_success() {
    Ok(())
  } else {
    Err(Error::Authentication(format!("{operation} was rejected")))
  }
}

fn parse_json<T: for<'de> Deserialize<'de>>(bytes: &[u8], label: &str) -> Result<T> {
  serde_json::from_slice(bytes).map_err(|_| Error::Authentication(format!("invalid {label}")))
}

#[cfg(test)]
#[path = "../tests/account_microsoft.rs"]
mod tests;
