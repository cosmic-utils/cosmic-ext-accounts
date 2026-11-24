use std::fmt;

use uuid::Uuid;
use zbus::{Connection, fdo::Result as FdoResult};

use crate::{
    error::{AccountsError, Result},
    models::{Account, Service},
    proxy::TasksProxy,
};

use super::{
    account::account_path,
    credentials::{access_token, validate_url},
};

/// Credentials for a tasks account, after broker authorization.
///
/// Secrets are redacted in `Debug` output.
#[derive(Clone, PartialEq, Eq)]
pub enum TasksCredentials {
    OAuth {
        uri: String,
        token: String,
        expires_at: i64,
    },
}

impl fmt::Debug for TasksCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OAuth { uri, .. } => f
                .debug_struct("OAuth")
                .field("uri", uri)
                .field("token", &"<redacted>")
                .finish(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TasksClient {
    connection: Connection,
    proxy: TasksProxy<'static>,
    account: Account,
}

impl TasksClient {
    pub async fn new(account: &Account) -> FdoResult<Self> {
        let connection = Connection::session().await?;
        let proxy = TasksProxy::new(&connection, account_path(&account.id)).await?;
        Ok(Self {
            connection,
            proxy,
            account: account.clone(),
        })
    }

    /// Connects using the shared error type instead of raw D-Bus errors.
    pub async fn connect(account: &Account) -> Result<Self> {
        Self::new(account).await.map_err(AccountsError::from)
    }

    pub async fn uri(&self) -> FdoResult<String> {
        self.proxy.uri().await
    }

    pub async fn auth_method(&self) -> FdoResult<String> {
        self.proxy.auth_method().await
    }

    /// Resolves the endpoint and access token in one call, including consent.
    pub async fn credentials(&self, refresh: bool) -> Result<TasksCredentials> {
        credentials(&self.connection, &self.account.id, refresh).await
    }
}

pub(super) async fn endpoint_proxy(
    connection: &Connection,
    id: &Uuid,
) -> Result<TasksProxy<'static>> {
    TasksProxy::builder(connection)
        .path(account_path(id))
        .map_err(AccountsError::from)?
        .build()
        .await
        .map_err(AccountsError::from)
}

/// Shared credential resolution used by both `TasksClient` and `AccountsClient`.
pub(super) async fn credentials(
    connection: &Connection,
    id: &Uuid,
    refresh: bool,
) -> Result<TasksCredentials> {
    let endpoint = endpoint_proxy(connection, id).await?;
    let uri = endpoint.uri().await.map_err(AccountsError::from)?;
    validate_url(&uri, "https://")?;

    let method = endpoint.auth_method().await.map_err(AccountsError::from)?;
    if method != "oauth2" {
        return Err(AccountsError::Unsupported);
    }

    let (token, expires_at) = access_token(connection, id, &Service::Tasks, refresh).await?;
    Ok(TasksCredentials::OAuth {
        uri,
        token,
        expires_at,
    })
}
