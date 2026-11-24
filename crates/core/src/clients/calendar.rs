use crate::{
    models::Account,
    proxy::{CalendarProxy, CredentialsProxy},
};
use zbus::{Connection, fdo::Result};

#[derive(Debug, Clone)]
pub struct CalendarClient {
    credentials: CredentialsProxy<'static>,
    proxy: CalendarProxy<'static>,
    account: Account,
}

impl CalendarClient {
    pub async fn new(account: &Account) -> Result<Self> {
        let connection = Connection::session().await?;
        let path = format!("/dev/edfloreshz/Accounts/Accounts/{}", account.dbus_id());
        let proxy = CalendarProxy::new(&connection, path.clone()).await?;
        let credentials = CredentialsProxy::builder(&connection)
            .path(path)
            .map_err(|err| zbus::fdo::Error::Failed(err.to_string()))?
            .build()
            .await?;
        Ok(Self {
            proxy,
            credentials,
            account: account.clone(),
        })
    }

    /// Requests the URL, username, and password together after Calendar consent.
    pub async fn caldav_access(&self) -> Result<(String, String, String)> {
        self.credentials.get_caldav_access().await
    }

    pub async fn uri(&self) -> Result<String> {
        self.proxy.uri().await
    }

    pub async fn auth_method(&self) -> Result<String> {
        self.proxy.auth_method().await
    }
}
