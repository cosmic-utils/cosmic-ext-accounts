use std::fmt;

use uuid::Uuid;
use zbus::{Connection, fdo::Result as FdoResult};

use crate::{
    error::{AccountsError, Result},
    models::{Account, Service},
    proxy::{CalendarAuthMethod, CalendarProxy},
};

use super::{
    account::account_path,
    credentials::{access_token, proxy, validate_password, validate_url, validate_value},
};

/// Credentials for a CalDAV account, after broker authorization.
///
/// Secrets are redacted in `Debug` output.
#[derive(Clone, PartialEq, Eq)]
pub enum CalendarCredentials {
    Basic {
        uri: String,
        username: String,
        password: String,
    },
    OAuth {
        uri: String,
        token: String,
        expires_at: i64,
    },
}

impl fmt::Debug for CalendarCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Basic { uri, username, .. } => f
                .debug_struct("Basic")
                .field("uri", uri)
                .field("username", username)
                .field("password", &"<redacted>")
                .finish(),
            Self::OAuth { uri, .. } => f
                .debug_struct("OAuth")
                .field("uri", uri)
                .field("token", &"<redacted>")
                .finish(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CalendarClient {
    connection: Connection,
    credentials_proxy: crate::proxy::CredentialsProxy<'static>,
    proxy: CalendarProxy<'static>,
    account: Account,
}

impl CalendarClient {
    pub async fn new(account: &Account) -> FdoResult<Self> {
        let connection = Connection::session().await?;
        let path = account_path(&account.id);
        let proxy = CalendarProxy::new(&connection, path.clone()).await?;
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

    /// Requests the URL, username, and password together after Calendar consent.
    pub async fn caldav_access(&self) -> FdoResult<(String, String, String)> {
        self.credentials_proxy.get_caldav_access().await
    }

    pub async fn uri(&self) -> FdoResult<String> {
        self.proxy.uri().await
    }

    pub async fn auth_method(&self) -> FdoResult<String> {
        self.proxy.auth_method().await
    }

    /// Parsed form of the endpoint's advertised authentication scheme.
    pub async fn auth_method_kind(&self) -> Result<CalendarAuthMethod> {
        parse_calendar_auth(&self.auth_method().await?)
    }

    /// Resolves settings and credentials in one call, including consent.
    pub async fn credentials(&self, refresh: bool) -> Result<CalendarCredentials> {
        credentials(&self.connection, &self.account.id, refresh).await
    }
}

pub(super) async fn endpoint_proxy(
    connection: &Connection,
    id: &Uuid,
) -> Result<CalendarProxy<'static>> {
    CalendarProxy::builder(connection)
        .path(account_path(id))
        .map_err(AccountsError::from)?
        .build()
        .await
        .map_err(AccountsError::from)
}

fn parse_calendar_auth(value: &str) -> Result<CalendarAuthMethod> {
    CalendarAuthMethod::parse(value).ok_or(AccountsError::Unsupported)
}

/// Shared credential resolution used by both `CalendarClient` and `AccountsClient`.
pub(super) async fn credentials(
    connection: &Connection,
    id: &Uuid,
    refresh: bool,
) -> Result<CalendarCredentials> {
    let endpoint = endpoint_proxy(connection, id).await?;
    let uri = endpoint.uri().await.map_err(AccountsError::from)?;
    validate_url(&uri, "https://")?;

    match parse_calendar_auth(&endpoint.auth_method().await.map_err(AccountsError::from)?)? {
        CalendarAuthMethod::Basic => {
            let (url, username, password) = proxy(connection, id)
                .await?
                .get_caldav_access()
                .await
                .map_err(AccountsError::from)?;
            validate_url(&url, "https://")?;
            validate_value(&username, AccountsError::Disabled)?;
            validate_password(&password, AccountsError::NeedsReauth)?;
            Ok(CalendarCredentials::Basic {
                uri: url,
                username,
                password,
            })
        }
        CalendarAuthMethod::OAuth2 => {
            let (token, expires_at) =
                access_token(connection, id, &Service::Calendar, refresh).await?;
            Ok(CalendarCredentials::OAuth {
                uri,
                token,
                expires_at,
            })
        }
    }
}
