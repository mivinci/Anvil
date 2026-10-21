use super::*;
use std::{
  collections::HashMap,
  sync::Mutex,
  time::{Duration, SystemTime},
};

const CLIENT_ID: &str = "01234567-89ab-cdef-0123-456789abcdef";
const ACCOUNT_ID: &str = "00112233445566778899aabbccddeeff";

#[derive(Default)]
struct MemoryCredentialStore(Mutex<HashMap<String, RefreshToken>>);

impl CredentialStore for MemoryCredentialStore {
  fn load_refresh_token(&self, account_id: &str) -> Result<Option<RefreshToken>> {
    self
      .0
      .lock()
      .unwrap()
      .get(account_id)
      .map(|token| RefreshToken::new(token.expose_secret()))
      .transpose()
  }

  fn store_refresh_token(&self, account_id: &str, token: RefreshToken) -> Result<()> {
    self.0.lock().unwrap().insert(account_id.to_owned(), token);
    Ok(())
  }

  fn delete_refresh_token(&self, account_id: &str) -> Result<()> {
    self.0.lock().unwrap().remove(account_id);
    Ok(())
  }
}

struct FixtureProvider;

impl AccountProvider for FixtureProvider {
  fn authorize(
    &self,
    client_id: MicrosoftClientId,
    _cancel: CancellationToken,
    events: mpsc::UnboundedSender<AuthorizationEvent>,
  ) -> AuthorizationFuture {
    Box::pin(async move {
      assert_eq!(client_id.as_str(), CLIENT_ID);
      events
        .send(AuthorizationEvent::DeviceCode {
          verification_uri: "https://microsoft.com/devicelogin".into(),
          user_code: "ABCD-EFGH".into(),
          expires_in_seconds: 900,
          poll_interval_seconds: 5,
        })
        .unwrap();
      authorized("refresh-token-1")
    })
  }

  fn refresh(
    &self,
    client_id: MicrosoftClientId,
    account_id: String,
    refresh_token: RefreshToken,
    _cancel: CancellationToken,
    _events: mpsc::UnboundedSender<AuthorizationEvent>,
  ) -> AuthorizationFuture {
    Box::pin(async move {
      assert_eq!(client_id.as_str(), CLIENT_ID);
      assert_eq!(account_id, ACCOUNT_ID);
      assert_eq!(refresh_token.expose_secret(), "refresh-token-1");
      authorized("refresh-token-2")
    })
  }
}

fn authorized(refresh_token: &str) -> Result<AuthenticatedAccount> {
  authorized_as(ACCOUNT_ID, "AnvilPlayer", refresh_token)
}

fn authorized_as(
  account_id: &str,
  username: &str,
  refresh_token: &str,
) -> Result<AuthenticatedAccount> {
  AuthenticatedAccount::new(
    AccountProfile::new(account_id, username, Some("2533274799999999".into()))?,
    AccessToken::new("access-token-secret")?,
    RefreshToken::new(refresh_token)?,
    SystemTime::now() + Duration::from_secs(3600),
    false,
  )
}

struct MismatchedRefreshProvider;

impl AccountProvider for MismatchedRefreshProvider {
  fn authorize(
    &self,
    _client_id: MicrosoftClientId,
    _cancel: CancellationToken,
    _events: mpsc::UnboundedSender<AuthorizationEvent>,
  ) -> AuthorizationFuture {
    Box::pin(async { authorized("unused") })
  }

  fn refresh(
    &self,
    _client_id: MicrosoftClientId,
    _account_id: String,
    _refresh_token: RefreshToken,
    _cancel: CancellationToken,
    _events: mpsc::UnboundedSender<AuthorizationEvent>,
  ) -> AuthorizationFuture {
    Box::pin(async {
      authorized_as(
        "11112222333344445555666677778888",
        "AnotherPlayer",
        "rotated",
      )
    })
  }
}

#[tokio::test]
async fn account_service_emits_device_code_rotates_refresh_token_and_logs_out() {
  let credentials = Arc::new(MemoryCredentialStore::default());
  let service = AccountService::new(
    MicrosoftClientId::new(CLIENT_ID).unwrap(),
    Arc::new(FixtureProvider),
    credentials.clone(),
  );
  let (events, mut received) = mpsc::unbounded_channel();
  let session = service
    .authorize(CancellationToken::new(), events)
    .await
    .unwrap();
  assert_eq!(session.profile().username(), "AnvilPlayer");
  assert_eq!(
    received.recv().await,
    Some(AuthorizationEvent::DeviceCode {
      verification_uri: "https://microsoft.com/devicelogin".into(),
      user_code: "ABCD-EFGH".into(),
      expires_in_seconds: 900,
      poll_interval_seconds: 5,
    })
  );
  assert!(!format!("{session:?}").contains("access-token-secret"));
  assert!(
    !format!("{:?}", RefreshToken::new("refresh-token-1").unwrap()).contains("refresh-token-1")
  );

  let profile = session.profile().clone();
  let resumed = service
    .resume(
      &profile,
      CancellationToken::new(),
      mpsc::unbounded_channel().0,
    )
    .await
    .unwrap();
  assert_eq!(resumed.profile(), &profile);
  assert!(!format!("{resumed:?}").contains("access-token-secret"));

  service.logout(profile.id()).await.unwrap();
  assert!(matches!(
    service
      .resume(
        &profile,
        CancellationToken::new(),
        mpsc::unbounded_channel().0
      )
      .await,
    Err(Error::AccountRequired)
  ));
  assert!(credentials.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn account_service_rejects_refresh_for_a_different_minecraft_identity() {
  let credentials = Arc::new(MemoryCredentialStore::default());
  credentials
    .store_refresh_token(ACCOUNT_ID, RefreshToken::new("refresh-token-1").unwrap())
    .unwrap();
  let service = AccountService::new(
    MicrosoftClientId::new(CLIENT_ID).unwrap(),
    Arc::new(MismatchedRefreshProvider),
    credentials.clone(),
  );
  let profile = AccountProfile::new(ACCOUNT_ID, "AnvilPlayer", None).unwrap();
  let result = service
    .resume(
      &profile,
      CancellationToken::new(),
      mpsc::unbounded_channel().0,
    )
    .await;
  assert!(matches!(result, Err(Error::Authentication(_))));
  assert_eq!(
    credentials
      .load_refresh_token(ACCOUNT_ID)
      .unwrap()
      .unwrap()
      .expose_secret(),
    "refresh-token-1"
  );
}

#[test]
fn profile_and_client_registration_are_validated_without_exposing_secrets() {
  assert!(MicrosoftClientId::new("Anvil").is_err());
  assert!(AccountProfile::new("--------00112233445566778899aabb", "player", None).is_err());
  let profile = AccountProfile::new(ACCOUNT_ID, "AnvilPlayer", Some("1234567890".into())).unwrap();
  let json = serde_json::to_string(&profile).unwrap();
  assert!(json.contains("AnvilPlayer"));
  assert!(!json.contains("token"));
}

#[test]
fn system_credential_store_rejects_invalid_account_ids_before_vault_access() {
  assert!(matches!(
    SystemCredentialStore.load_refresh_token("../not-an-account"),
    Err(Error::Invalid(_))
  ));
}
