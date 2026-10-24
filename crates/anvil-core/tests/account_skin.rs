use super::*;

const ID: &str = "00112233445566778899aabbccddeeff";
fn profile(url: &str, model: &str) -> Vec<u8> {
  let value = serde_json::json!({"textures":{"SKIN":{"url":url,"metadata":{"model":model}}}});
  serde_json::to_vec(&serde_json::json!({"id":ID,"properties":[{"name":"textures","value":STANDARD.encode(value.to_string())}]})).unwrap()
}

#[test]
fn skin_accepts_official_texture_and_upgrades_https() {
  let url = format!("http://textures.minecraft.net/texture/{}", "a".repeat(64));
  let skin = parse(&profile(&url, "slim"), ID).unwrap().unwrap();
  assert_eq!(skin.model, "slim");
  assert!(
    skin
      .url
      .starts_with("https://textures.minecraft.net/texture/")
  );
}

#[test]
fn skin_rejects_identity_mismatch_and_untrusted_texture() {
  let url = format!("https://textures.minecraft.net/texture/{}", "a".repeat(64));
  assert!(parse(&profile(&url, "classic"), "another-account").is_err());
  for bad in [
    "https://example.com/texture/abc",
    "https://textures.minecraft.net@evil.test/texture/abc",
    "file:///skin.png",
  ] {
    assert!(parse(&profile(bad, "classic"), ID).is_err());
  }
  assert!(parse(&profile(&url, "unknown"), ID).is_err());
  assert!(parse(br#"{"id":"00112233445566778899aabbccddeeff","properties":[{"name":"textures","value":"invalid!"}]}"#, ID).is_err());
}

#[test]
fn skin_handles_profile_without_custom_skin() {
  let bytes = serde_json::to_vec(&serde_json::json!({"id":ID,"properties":[]})).unwrap();
  assert!(parse(&bytes, ID).unwrap().is_none());
}

#[tokio::test]
async fn skin_restores_fresh_cache_without_network() {
  let directory = tempfile::tempdir().unwrap();
  let context =
    Context::new(crate::Paths::discover(Some(directory.path().to_owned())).unwrap()).unwrap();
  let skin = AccountSkin {
    url: format!("https://textures.minecraft.net/texture/{}", "b".repeat(64)),
    model: "classic".into(),
  };
  catalog_cache::save(
    &context,
    catalog_cache::ACCOUNT_SKINS,
    ID,
    Some(skin.clone()),
    SystemTime::now(),
  )
  .await
  .unwrap();
  assert_eq!(read(&context, ID).await.unwrap().unwrap().url, skin.url);
  assert!(read(&context, "../bad").await.is_err());
}
