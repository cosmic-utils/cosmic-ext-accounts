use crate::{
    models::{Account, ContactsConnectionSettings},
    proxy::{ContactsProxy, CredentialsProxy},
};
use zbus::{Connection, fdo::Result};

#[derive(Debug, Clone)]
pub struct ContactsClient {
    proxy: ContactsProxy<'static>,
    credentials: CredentialsProxy<'static>,
}

impl ContactsClient {
    pub async fn new(account: &Account) -> Result<Self> {
        let connection = Connection::session().await?;
        let path = format!("/dev/edfloreshz/Accounts/Accounts/{}", account.dbus_id());
        let proxy = ContactsProxy::new(&connection, path.clone()).await?;
        let credentials = CredentialsProxy::builder(&connection)
            .path(path)
            .map_err(|err| zbus::fdo::Error::Failed(err.to_string()))?
            .build()
            .await?;
        Ok(Self { proxy, credentials })
    }

    pub async fn connection_settings(&self) -> Result<ContactsConnectionSettings> {
        self.proxy.get_connection_settings().await
    }

    /// Requests validated server settings, username, and password after Contacts consent.
    pub async fn contact_access(&self) -> Result<(ContactsConnectionSettings, String, String)> {
        self.credentials.get_contact_access().await
    }

    pub async fn uri(&self) -> Result<String> {
        self.proxy.uri().await
    }

    pub async fn auth_method(&self) -> Result<String> {
        self.proxy.auth_method().await
    }
}
