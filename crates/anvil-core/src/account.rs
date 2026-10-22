//! Minecraft account identities, Microsoft/Xbox authentication, and credential storage.
use std::{fmt, future::Future, pin::Pin, sync::Arc, time::SystemTime};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{Error, Result};

pub type AuthorizationFuture =
  Pin<Box<dyn Future<Output = Result<AuthenticatedAccount>> + Send + 'static>>;

#[path = "microsoft.rs"]
pub mod microsoft;

pub use microsoft::{MicrosoftAccountProvider, SystemCredentialStore};

/// Public Minecraft identity data. This can be stored in `accounts.json`; it
/// deliberately contains no OAuth or Minecraft access token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountProfile {
  id: String,
  username: String,
  xuid: Option<String>,
}

impl AccountProfile {
  pub fn new(
    id: impl Into<String>,
    username: impl Into<String>,
    xuid: Option<String>,
  ) -> Result<Self> {
    let profile = Self {
      id: id.into(),
      username: username.into(),
      xuid,
    };
    profile.validate()?;
    Ok(profile)
  }

  pub fn id(&self) -> &str {
    &self.id
  }

  pub fn username(&self) -> &str {
    &self.username
  }

  pub fn xuid(&self) -> Option<&str> {
    self.xuid.as_deref()
  }

  pub fn validate(&self) -> Result<()> {
    if !valid_uuid(&self.id)
      || self.username.is_empty()
      || self.username.len() > 16
      || !self
        .username
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
      || self.xuid.as_ref().is_some_and(|xuid| {
        xuid.is_empty() || xuid.len() > 20 || !xuid.bytes().all(|byte| byte.is_ascii_digit())
      })
    {
      return Err(Error::Authentication(
        "Minecraft profile has invalid identity fields".into(),
      ));
    }
    Ok(())
  }
}

/// Azure application registration ID. It must come from Anvil's registered
/// Microsoft application and is never inferred from the launcher name.
#[derive(Clone, PartialEq, Eq)]
pub struct MicrosoftClientId(String);

impl MicrosoftClientId {
  pub fn new(value: impl Into<String>) -> Result<Self> {
    let value = value.into();
    if !valid_guid(&value) {
      return Err(Error::Invalid(
        "Microsoft OAuth client ID must be a GUID".into(),
      ));
    }
    Ok(Self(value.to_ascii_lowercase()))
  }

  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Debug for MicrosoftClientId {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_tuple("MicrosoftClientId")
      .field(&self.0)
      .finish()
  }
}

macro_rules! secret_type {
  ($type:ident, $label:literal) => {
    /// Opaque secret value. It is not serializable and its Debug output is redacted.
    pub struct $type(String);

    impl $type {
      pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty()
          || value.len() > 64 * 1024
          || !value.bytes().all(|byte| byte.is_ascii_graphic())
        {
          return Err(Error::Authentication(concat!("invalid ", $label).into()));
        }
        Ok(Self(value))
      }

      pub fn expose_secret(&self) -> &str {
        &self.0
      }
    }

    impl fmt::Debug for $type {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(concat!($label, "([REDACTED])"))
      }
    }
  };
}

secret_type!(AccessToken, "AccessToken");
secret_type!(RefreshToken, "RefreshToken");

/// User-visible progress from a device-code flow. Device tokens remain private
/// inside the provider and must never be sent to the terminal or GUI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthorizationEvent {
  DeviceCode {
    verification_uri: String,
    user_code: String,
    expires_in_seconds: u64,
    poll_interval_seconds: u64,
  },
  WaitingForApproval,
  /// Microsoft approved the device; Xbox and Minecraft identity checks are underway.
  CheckingProfile,
}

/// Result of a successful Microsoft → Xbox → Minecraft profile exchange.
/// The account service persists only the refresh token in the credential vault.
pub struct AuthenticatedAccount {
  profile: AccountProfile,
  access_token: AccessToken,
  refresh_token: RefreshToken,
  expires_at: SystemTime,
  demo_user: bool,
}

impl AuthenticatedAccount {
  pub fn new(
    profile: AccountProfile,
    access_token: AccessToken,
    refresh_token: RefreshToken,
    expires_at: SystemTime,
    demo_user: bool,
  ) -> Result<Self> {
    profile.validate()?;
    if expires_at <= SystemTime::now() {
      return Err(Error::Authentication(
        "provider returned an expired Minecraft session".into(),
      ));
    }
    Ok(Self {
      profile,
      access_token,
      refresh_token,
      expires_at,
      demo_user,
    })
  }
}

/// Short-lived session returned to the launch planner. It cannot be serialized;
/// its Debug output omits the access token.
pub struct AccountSession {
  profile: AccountProfile,
  access_token: AccessToken,
  expires_at: SystemTime,
  demo_user: bool,
}

impl AccountSession {
  pub fn profile(&self) -> &AccountProfile {
    &self.profile
  }

  pub fn expires_at(&self) -> SystemTime {
    self.expires_at
  }

  pub fn is_demo_user(&self) -> bool {
    self.demo_user
  }

  pub(crate) fn access_token(&self) -> &str {
    self.access_token.expose_secret()
  }

  fn from_authenticated(account: AuthenticatedAccount) -> (Self, RefreshToken) {
    (
      Self {
        profile: account.profile,
        access_token: account.access_token,
        expires_at: account.expires_at,
        demo_user: account.demo_user,
      },
      account.refresh_token,
    )
  }
}

impl fmt::Debug for AccountSession {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("AccountSession")
      .field("profile", &self.profile)
      .field("access_token", &"[REDACTED]")
      .field("expires_at", &self.expires_at)
      .field("demo_user", &self.demo_user)
      .finish()
  }
}

/// UI-independent authorization protocol adapter. A CLI can render
/// `DeviceCode` events while a GUI can present the same event in a dialog.
pub trait AccountProvider: Send + Sync + 'static {
  fn authorize(
    &self,
    client_id: MicrosoftClientId,
    cancel: CancellationToken,
    events: mpsc::UnboundedSender<AuthorizationEvent>,
  ) -> AuthorizationFuture;

  fn refresh(
    &self,
    client_id: MicrosoftClientId,
    account_id: String,
    refresh_token: RefreshToken,
    cancel: CancellationToken,
    events: mpsc::UnboundedSender<AuthorizationEvent>,
  ) -> AuthorizationFuture;
}

/// Platform keychain/credential-manager boundary. Adapters should store opaque
/// refresh tokens only; profiles belong in regular application data.
pub trait CredentialStore: Send + Sync + 'static {
  fn load_refresh_token(&self, account_id: &str) -> Result<Option<RefreshToken>>;
  fn store_refresh_token(&self, account_id: &str, token: RefreshToken) -> Result<()>;
  fn delete_refresh_token(&self, account_id: &str) -> Result<()>;
}

/// Coordinates provider calls and refresh-token rotation without depending on
/// a GUI, terminal, or a particular operating-system credential store.
pub struct AccountService<P, V> {
  client_id: MicrosoftClientId,
  provider: Arc<P>,
  credentials: Arc<V>,
}

impl<P, V> AccountService<P, V>
where
  P: AccountProvider,
  V: CredentialStore,
{
  pub fn new(client_id: MicrosoftClientId, provider: Arc<P>, credentials: Arc<V>) -> Self {
    Self {
      client_id,
      provider,
      credentials,
    }
  }

  pub async fn authorize(
    &self,
    cancel: CancellationToken,
    events: mpsc::UnboundedSender<AuthorizationEvent>,
  ) -> Result<AccountSession> {
    if cancel.is_cancelled() {
      return Err(Error::Cancelled);
    }
    let authenticated = self
      .provider
      .authorize(self.client_id.clone(), cancel.clone(), events)
      .await?;
    if cancel.is_cancelled() {
      return Err(Error::Cancelled);
    }
    let (session, refresh_token) = AccountSession::from_authenticated(authenticated);
    store_token(
      self.credentials.clone(),
      session.profile.id.clone(),
      refresh_token,
    )
    .await?;
    Ok(session)
  }

  pub async fn resume(
    &self,
    profile: &AccountProfile,
    cancel: CancellationToken,
    events: mpsc::UnboundedSender<AuthorizationEvent>,
  ) -> Result<AccountSession> {
    profile.validate()?;
    if cancel.is_cancelled() {
      return Err(Error::Cancelled);
    }
    let account_id = profile.id.clone();
    let credentials = self.credentials.clone();
    let refresh_token =
      tokio::task::spawn_blocking(move || credentials.load_refresh_token(&account_id))
        .await??
        .ok_or(Error::AccountRequired)?;
    let authenticated = self
      .provider
      .refresh(
        self.client_id.clone(),
        profile.id.clone(),
        refresh_token,
        cancel.clone(),
        events,
      )
      .await?;
    if cancel.is_cancelled() {
      return Err(Error::Cancelled);
    }
    if authenticated.profile.id != profile.id {
      return Err(Error::Authentication(
        "refreshed token belongs to a different Minecraft account".into(),
      ));
    }
    let (session, refresh_token) = AccountSession::from_authenticated(authenticated);
    store_token(self.credentials.clone(), profile.id.clone(), refresh_token).await?;
    Ok(session)
  }

  pub async fn logout(&self, account_id: &str) -> Result<()> {
    if !valid_uuid(account_id) {
      return Err(Error::Invalid("invalid Minecraft account ID".into()));
    }
    let account_id = account_id.to_owned();
    let credentials = self.credentials.clone();
    tokio::task::spawn_blocking(move || credentials.delete_refresh_token(&account_id)).await?
  }
}

async fn store_token<V: CredentialStore>(
  credentials: Arc<V>,
  account_id: String,
  refresh_token: RefreshToken,
) -> Result<()> {
  tokio::task::spawn_blocking(move || credentials.store_refresh_token(&account_id, refresh_token))
    .await?
}

fn valid_uuid(value: &str) -> bool {
  if value.len() == 32 {
    return value.bytes().all(|byte| byte.is_ascii_hexdigit());
  }
  value.len() == 36
    && value.bytes().enumerate().all(|(index, byte)| {
      if matches!(index, 8 | 13 | 18 | 23) {
        byte == b'-'
      } else {
        byte.is_ascii_hexdigit()
      }
    })
}

fn valid_guid(value: &str) -> bool {
  value.len() == 36
    && value.bytes().enumerate().all(|(index, byte)| {
      if matches!(index, 8 | 13 | 18 | 23) {
        byte == b'-'
      } else {
        byte.is_ascii_hexdigit()
      }
    })
}

#[cfg(test)]
#[path = "../tests/account.rs"]
mod tests;
