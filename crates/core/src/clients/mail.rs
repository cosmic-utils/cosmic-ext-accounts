use std::fmt;

use uuid::Uuid;
use zbus::{Connection, fdo::Result as FdoResult};

use crate::{
    error::{AccountsError, Result},
    models::Account,
    proxy::{CredentialsProxy, MailAuthMethod, MailConnectionSettings, MailProxy},
};

use super::account::account_path;

/// Credentials for an IMAP/SMTP account, after broker authorization.
///
/// Secrets are redacted in `Debug` output.
#[derive(Clone, PartialEq, Eq)]
pub enum MailCredentials {
    OAuth {
        settings: MailConnectionSettings,
        token: String,
        expires_at: i64,
    },
    Password {
        settings: MailConnectionSettings,
        username: String,
        password: String,
    },
}

impl fmt::Debug for MailCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OAuth { settings, .. } => f
                .debug_struct("OAuth")
                .field("settings", settings)
                .field("token", &"<redacted>")
                .finish(),
            Self::Password {
                settings, username, ..
            } => f
                .debug_struct("Password")
                .field("settings", settings)
                .field("username", username)
                .field("password", &"<redacted>")
                .finish(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MailClient {
    connection: Connection,
    proxy: MailProxy<'static>,
    account: Account,
}

impl MailClient {
    pub async fn new(account: &Account) -> FdoResult<Self> {
        let connection = Connection::session().await?;
        let proxy = MailProxy::new(
            &connection,
            format!("/dev/edfloreshz/Accounts/Accounts/{}", account.dbus_id()),
        )
        .await?;
        Ok(Self {
            connection,
            proxy,
            account: account.clone(),
        })
    }

    pub async fn connection_settings(&self) -> FdoResult<MailConnectionSettings> {
        self.proxy.get_connection_settings().await
    }

    pub async fn imap_host(&self) -> FdoResult<String> {
        self.proxy.imap_host().await
    }

    pub async fn imap_port(&self) -> FdoResult<u16> {
        self.proxy.imap_port().await
    }

    pub async fn smtp_host(&self) -> FdoResult<String> {
        self.proxy.smtp_host().await
    }

    pub async fn smtp_port(&self) -> FdoResult<u16> {
        self.proxy.smtp_port().await
    }

    pub async fn auth_method(&self) -> FdoResult<String> {
        self.proxy.auth_method().await
    }

    /// Resolves settings and credentials in one call, including consent and
    /// token refresh when requested.
    pub async fn credentials(&self, refresh: bool) -> Result<MailCredentials> {
        mail_credentials(&self.connection, &self.account.id, refresh).await
    }
}

pub(super) async fn credentials_proxy(
    connection: &Connection,
    id: &Uuid,
) -> Result<CredentialsProxy<'static>> {
    CredentialsProxy::builder(connection)
        .path(account_path(id))
        .map_err(AccountsError::from)?
        .build()
        .await
        .map_err(AccountsError::from)
}

/// Shared implementation used by both `AccountsClient` and `MailClient`.
pub(super) async fn mail_credentials(
    connection: &Connection,
    id: &Uuid,
    refresh: bool,
) -> Result<MailCredentials> {
    let endpoint = MailProxy::builder(connection)
        .path(account_path(id))
        .map_err(AccountsError::from)?
        .build()
        .await
        .map_err(AccountsError::from)?;

    let advertised = endpoint
        .get_connection_settings()
        .await
        .map_err(AccountsError::from)?;
    if !advertised.is_supported() {
        return Err(AccountsError::Disabled);
    }

    let credentials = credentials_proxy(connection, id).await?;
    match advertised.auth_method() {
        Some(MailAuthMethod::Password) => {
            let (settings, username, password) = credentials
                .get_mail_password()
                .await
                .map_err(AccountsError::from)?;
            if !settings.is_supported()
                || settings.auth_method() != Some(MailAuthMethod::Password)
                || username.is_empty()
                || username.chars().any(char::is_control)
                || password.is_empty()
                || password.chars().any(|c| matches!(c, '\r' | '\n' | '\0'))
            {
                return Err(AccountsError::Disabled);
            }
            Ok(MailCredentials::Password {
                settings,
                username,
                password,
            })
        }
        Some(MailAuthMethod::OAuth2) => {
            if refresh {
                credentials
                    .invalidate_token()
                    .await
                    .map_err(AccountsError::from)?;
            }
            let (settings, token, expires_at) = credentials
                .get_mail_access()
                .await
                .map_err(AccountsError::from)?;
            if token.is_empty() || token.chars().any(char::is_control) {
                return Err(AccountsError::NeedsReauth);
            }
            if !settings.is_supported() || settings.auth_method() != Some(MailAuthMethod::OAuth2) {
                return Err(AccountsError::Disabled);
            }
            Ok(MailCredentials::OAuth {
                settings,
                token,
                expires_at,
            })
        }
        None => Err(AccountsError::Unsupported),
    }
}
