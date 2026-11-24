use std::fmt;

use uuid::Uuid;
use zbus::{Connection, fdo::Result as FdoResult};

use crate::{
    error::{AccountsError, Result},
    models::{Account, ContactsConnectionSettings, Service},
    proxy::{ContactsAuthMethod, ContactsProxy},
};

use super::{
    account::account_path,
    credentials::{access_token, proxy, validate_password, validate_value},
};

/// Credentials for a contacts account, after broker authorization.
///
/// Secrets are redacted in `Debug` output.
#[derive(Clone, PartialEq, Eq)]
pub enum ContactsCredentials {
    Password {
        settings: ContactsConnectionSettings,
        username: String,
        password: String,
    },
    OAuth {
        settings: ContactsConnectionSettings,
        token: String,
        expires_at: i64,
    },
}

impl fmt::Debug for ContactsCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Password {
                settings, username, ..
            } => f
                .debug_struct("Password")
                .field("settings", settings)
                .field("username", username)
                .field("password", &"<redacted>")
                .finish(),
            Self::OAuth { settings, .. } => f
                .debug_struct("OAuth")
                .field("settings", settings)
                .field("token", &"<redacted>")
                .finish(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContactsClient {
    connection: Connection,
    proxy: ContactsProxy<'static>,
    credentials_proxy: crate::proxy::CredentialsProxy<'static>,
    account: Account,
}

impl ContactsClient {
    pub async fn new(account: &Account) -> FdoResult<Self> {
        let connection = Connection::session().await?;
        let path = account_path(&account.id);
        let proxy = ContactsProxy::new(&connection, path.clone()).await?;
        let credentials_proxy = crate::proxy::CredentialsProxy::builder(&connection)
            .path(path)
            .map_err(|err| zbus::fdo::Error::Failed(err.to_string()))?
            .build()
            .await?;
        Ok(Self {
            connection,
            proxy,
            credentials_proxy,
            account: account.clone(),
        })
    }

    /// Connects using the shared error type instead of raw D-Bus errors.
    pub async fn connect(account: &Account) -> Result<Self> {
        Self::new(account).await.map_err(AccountsError::from)
    }

    pub async fn connection_settings(&self) -> FdoResult<ContactsConnectionSettings> {
        self.proxy.get_connection_settings().await
    }

    /// Requests validated server settings, username, and password after Contacts consent.
    pub async fn contact_access(&self) -> FdoResult<(ContactsConnectionSettings, String, String)> {
        self.credentials_proxy.get_contact_access().await
    }

    pub async fn uri(&self) -> FdoResult<String> {
        self.proxy.uri().await
    }

    pub async fn auth_method(&self) -> FdoResult<String> {
        self.proxy.auth_method().await
    }

    /// Parsed form of the endpoint's advertised authentication scheme.
    pub async fn auth_method_kind(&self) -> Result<ContactsAuthMethod> {
        parse_contacts_auth(&self.auth_method().await?)
    }

    /// Resolves settings and credentials in one call, including consent.
    pub async fn credentials(&self, refresh: bool) -> Result<ContactsCredentials> {
        credentials(&self.connection, &self.account.id, refresh).await
    }
}

pub(super) async fn endpoint_proxy(
    connection: &Connection,
    id: &Uuid,
) -> Result<ContactsProxy<'static>> {
    ContactsProxy::builder(connection)
        .path(account_path(id))
        .map_err(AccountsError::from)?
        .build()
        .await
        .map_err(AccountsError::from)
}

pub(super) async fn connection_settings(
    connection: &Connection,
    id: &Uuid,
) -> Result<ContactsConnectionSettings> {
    let settings = endpoint_proxy(connection, id)
        .await?
        .get_connection_settings()
        .await
        .map_err(AccountsError::from)?;
    settings.validate().map_err(|_| AccountsError::Disabled)?;
    Ok(settings)
}

fn parse_contacts_auth(value: &str) -> Result<ContactsAuthMethod> {
    ContactsAuthMethod::parse(value).ok_or(AccountsError::Unsupported)
}

/// Shared credential resolution used by both `ContactsClient` and `AccountsClient`.
pub(super) async fn credentials(
    connection: &Connection,
    id: &Uuid,
    refresh: bool,
) -> Result<ContactsCredentials> {
    let endpoint = endpoint_proxy(connection, id).await?;
    let advertised = endpoint
        .get_connection_settings()
        .await
        .map_err(AccountsError::from)?;
    advertised.validate().map_err(|_| AccountsError::Disabled)?;
    let method = parse_contacts_auth(&endpoint.auth_method().await.map_err(AccountsError::from)?)?;

    match method {
        ContactsAuthMethod::Basic | ContactsAuthMethod::SimpleBind => {
            let (settings, username, password) = proxy(connection, id)
                .await?
                .get_contact_access()
                .await
                .map_err(AccountsError::from)?;
            settings.validate().map_err(|_| AccountsError::Disabled)?;
            if settings.protocol != advertised.protocol {
                return Err(AccountsError::Disabled);
            }
            validate_value(&username, AccountsError::Disabled)?;
            validate_password(&password, AccountsError::NeedsReauth)?;
            Ok(ContactsCredentials::Password {
                settings,
                username,
                password,
            })
        }
        ContactsAuthMethod::OAuth2 => {
            let (token, expires_at) =
                access_token(connection, id, &Service::Contacts, refresh).await?;
            Ok(ContactsCredentials::OAuth {
                settings: advertised,
                token,
                expires_at,
            })
        }
    }
}
