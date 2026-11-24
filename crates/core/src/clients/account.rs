use std::collections::HashMap;
use std::str::FromStr;

use crate::{
    error::{AccountsError, Result as AccountsResult},
    models::{Account, ContactsConnectionSettings, DbusProviderInfo, Provider, Service},
    proxy::{
        AccountAddedStream, AccountProxy, AccountRemovedStream, AccountToggledStream,
        CredentialsProxy, MailConnectionSettings, MailProxy, ManagerProxy, ProviderProxy,
        RequestProxy,
    },
};
use uuid::Uuid;
use zbus::{Connection, fdo::Result, zvariant::OwnedObjectPath};

use super::{
    calendar::{self, CalendarCredentials},
    contacts::{self, ContactsCredentials},
    credentials,
    mail::{self, MailCredentials},
    tasks::{self, TasksCredentials},
};

pub(super) fn account_path(id: &Uuid) -> OwnedObjectPath {
    OwnedObjectPath::try_from(format!(
        "/dev/edfloreshz/Accounts/Accounts/{}",
        id.to_string().replace('-', "_")
    ))
    .expect("account object path is always a valid path")
}

#[derive(Debug, Clone)]
pub struct AccountsClient {
    connection: Connection,
    manager: ManagerProxy<'static>,
}

impl AccountsClient {
    pub async fn new() -> Result<Self> {
        let connection = Connection::session().await?;
        let manager = ManagerProxy::new(&connection).await?;
        Ok(Self {
            connection,
            manager,
        })
    }

    /// Connects to the broker using the shared error type.
    ///
    /// This is the constructor consumers should use when they want
    /// `AccountsError` rather than the raw `zbus` error from `new()`.
    pub async fn connect() -> AccountsResult<Self> {
        Self::new().await.map_err(AccountsError::from)
    }

    async fn account_proxy(&self, path: OwnedObjectPath) -> Result<AccountProxy<'static>> {
        let builder = AccountProxy::builder(&self.connection)
            .path(path)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        builder
            .build()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    pub async fn request_proxy(&self, path: OwnedObjectPath) -> Result<RequestProxy<'static>> {
        let builder = RequestProxy::builder(&self.connection)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .path(path)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        builder
            .build()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn account_from_proxy(proxy: &AccountProxy<'static>) -> Result<Account> {
        let id = Uuid::from_str(&proxy.id().await?)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        let provider = proxy.provider_id().await?;
        let display_name = proxy.display_name().await?;
        let identity = proxy.identity().await?;
        let enabled = proxy.enabled().await?;
        let available_services = proxy.available_services().await?;
        let enabled_services = proxy.enabled_services().await?;
        let created_at = proxy.created_at().await.unwrap_or_default();
        let last_used = proxy.last_used().await.unwrap_or_default();
        let email = proxy.email().await.unwrap_or_default();

        let services = available_services
            .into_iter()
            .filter_map(Service::from_str)
            .map(|service| {
                let enabled = enabled_services.contains(&service.to_string());
                (service, enabled)
            })
            .collect();

        let custom_mail = if provider == crate::models::CUSTOM_MAIL_PROVIDER {
            Some(proxy.get_custom_mail_settings().await?)
        } else {
            None
        };

        let caldav_url = if provider == crate::models::CUSTOM_CALDAV_PROVIDER {
            Some(proxy.get_caldav_url().await?)
        } else {
            None
        };

        let custom_contacts = if matches!(
            provider.as_str(),
            crate::models::CUSTOM_CARDDAV_PROVIDER | crate::models::CUSTOM_LDAP_PROVIDER
        ) {
            Some(proxy.get_custom_contacts_settings().await?)
        } else {
            None
        };

        Ok(Account {
            id,
            provider,
            display_name,
            username: identity,
            email: (!email.is_empty()).then_some(email),
            enabled,
            created_at: chrono::DateTime::from_str(&created_at)
                .unwrap_or_else(|_| chrono::Utc::now()),
            last_used: chrono::DateTime::from_str(&last_used).ok(),
            services,
            custom_mail,
            caldav_url,
            custom_contacts,
        })
    }
}

impl AccountsClient {
    pub async fn list_accounts(&self) -> Result<Vec<Account>> {
        let paths = self.manager.list_accounts().await?;
        let mut accounts = Vec::with_capacity(paths.len());
        for path in paths {
            let proxy = self.account_proxy(path).await?;
            accounts.push(Self::account_from_proxy(&proxy).await?);
        }
        Ok(accounts)
    }

    pub async fn list_enabled_accounts(&self, service: Service) -> Result<Vec<Account>> {
        let accounts = self.list_accounts().await?;
        Ok(accounts
            .into_iter()
            .filter(|account| account.is_enabled_for(&service))
            .collect())
    }

    /// Lists accounts for service discovery.
    ///
    /// Unlike `list_accounts`, a single broken account object is skipped
    /// instead of failing discovery for every other account.
    async fn list_discoverable_accounts(&self) -> AccountsResult<Vec<Account>> {
        let paths = self
            .manager
            .list_accounts()
            .await
            .map_err(AccountsError::from)?;
        let mut accounts = Vec::with_capacity(paths.len());
        for path in paths {
            let Ok(proxy) = self.account_proxy(path).await else {
                continue;
            };
            let Ok(account) = Self::account_from_proxy(&proxy).await else {
                continue;
            };
            accounts.push(account);
        }
        Ok(accounts)
    }

    /// Version of the mail endpoint advertised by the daemon.
    pub async fn mail_endpoint_version(&self) -> AccountsResult<u32> {
        self.manager
            .mail_endpoint_version()
            .await
            .map_err(AccountsError::from)
    }

    /// Lists accounts usable for mail, with their validated endpoints.
    ///
    /// This performs the discovery/filtering every mail consumer would
    /// otherwise reimplement: enabled accounts with the Email service, a
    /// usable email identity and supported IMAP/SMTP settings.
    pub async fn list_mail_accounts(
        &self,
    ) -> AccountsResult<Vec<(Account, MailConnectionSettings)>> {
        if self.mail_endpoint_version().await? == 0 {
            return Err(AccountsError::UpgradeRequired);
        }

        let mut result = Vec::new();
        for account in self.list_discoverable_accounts().await? {
            if !account.is_enabled_for(&Service::Email) {
                continue;
            }
            let email = account.email_or_identity();
            if email.is_empty() || email.chars().any(char::is_control) {
                continue;
            }
            let Ok(endpoint) = MailProxy::builder(&self.connection)
                .path(account_path(&account.id))
                .map_err(AccountsError::from)
            else {
                continue;
            };
            let Ok(endpoint) = endpoint.build().await else {
                continue;
            };
            let Ok(settings) = endpoint.get_connection_settings().await else {
                continue;
            };
            if settings.is_supported() {
                result.push((account, settings));
            }
        }
        Ok(result)
    }

    /// Returns validated mail settings and an OAuth access token.
    pub async fn get_mail_access(
        &self,
        id: &Uuid,
        refresh: bool,
    ) -> AccountsResult<(MailConnectionSettings, String, i64)> {
        let credentials = credentials::proxy(&self.connection, id).await?;
        if refresh {
            credentials
                .invalidate_token()
                .await
                .map_err(AccountsError::from)?;
        }
        credentials
            .get_mail_access()
            .await
            .map_err(AccountsError::from)
    }

    /// Returns validated mail settings, login name and custom-mail password.
    pub async fn get_mail_password(
        &self,
        id: &Uuid,
    ) -> AccountsResult<(MailConnectionSettings, String, String)> {
        credentials::proxy(&self.connection, id)
            .await?
            .get_mail_password()
            .await
            .map_err(AccountsError::from)
    }

    /// Marks an OAuth token stale so the next request refreshes it.
    ///
    /// Kept as an alias for callers that think in terms of Mail only.
    pub async fn invalidate_mail_token(&self, id: &Uuid) -> AccountsResult<()> {
        self.invalidate_token(id).await
    }

    /// Resolves settings and credentials in one call, including consent.
    pub async fn mail_credentials(
        &self,
        id: &Uuid,
        refresh: bool,
    ) -> AccountsResult<MailCredentials> {
        mail::mail_credentials(&self.connection, id, refresh).await
    }

    /// Marks an OAuth token stale so the next request refreshes it.
    pub async fn invalidate_token(&self, id: &Uuid) -> AccountsResult<()> {
        credentials::proxy(&self.connection, id)
            .await?
            .invalidate_token()
            .await
            .map_err(AccountsError::from)
    }

    /// Lists accounts usable for calendar, with validated CalDAV endpoints.
    pub async fn list_calendar_accounts(&self) -> AccountsResult<Vec<(Account, String)>> {
        let mut result = Vec::new();
        for account in self.list_discoverable_accounts().await? {
            if !account.is_enabled_for(&Service::Calendar) {
                continue;
            }
            let Ok(endpoint) = calendar::endpoint_proxy(&self.connection, &account.id).await else {
                continue;
            };
            let Ok(uri) = endpoint.uri().await else {
                continue;
            };
            if credentials::validate_url(&uri, "https://").is_ok() {
                result.push((account, uri));
            }
        }
        Ok(result)
    }

    /// Resolves a CalDAV endpoint and its credentials in one call.
    pub async fn calendar_credentials(
        &self,
        id: &Uuid,
        refresh: bool,
    ) -> AccountsResult<CalendarCredentials> {
        calendar::credentials(&self.connection, id, refresh).await
    }

    /// Lists accounts usable for contacts, with validated directory settings.
    pub async fn list_contacts_accounts(
        &self,
    ) -> AccountsResult<Vec<(Account, ContactsConnectionSettings)>> {
        let mut result = Vec::new();
        for account in self.list_discoverable_accounts().await? {
            if !account.is_enabled_for(&Service::Contacts) {
                continue;
            }
            let Ok(settings) = contacts::connection_settings(&self.connection, &account.id).await
            else {
                continue;
            };
            result.push((account, settings));
        }
        Ok(result)
    }

    /// Resolves directory settings and credentials in one call.
    pub async fn contacts_credentials(
        &self,
        id: &Uuid,
        refresh: bool,
    ) -> AccountsResult<ContactsCredentials> {
        contacts::credentials(&self.connection, id, refresh).await
    }

    /// Lists accounts usable for tasks, with validated CalDAV endpoints.
    pub async fn list_tasks_accounts(&self) -> AccountsResult<Vec<(Account, String)>> {
        let mut result = Vec::new();
        for account in self.list_discoverable_accounts().await? {
            if !account.is_enabled_for(&Service::Tasks) {
                continue;
            }
            let Ok(endpoint) = tasks::endpoint_proxy(&self.connection, &account.id).await else {
                continue;
            };
            let Ok(uri) = endpoint.uri().await else {
                continue;
            };
            if credentials::validate_url(&uri, "https://").is_ok() {
                result.push((account, uri));
            }
        }
        Ok(result)
    }

    /// Resolves a tasks endpoint and its access token in one call.
    pub async fn tasks_credentials(
        &self,
        id: &Uuid,
        refresh: bool,
    ) -> AccountsResult<TasksCredentials> {
        tasks::credentials(&self.connection, id, refresh).await
    }

    /// Requests an OAuth token for any service, optionally forcing a refresh.
    pub async fn access_token(
        &self,
        id: &Uuid,
        service: &Service,
        refresh: bool,
    ) -> AccountsResult<(String, i64)> {
        credentials::access_token(&self.connection, id, service, refresh).await
    }

    pub async fn create_mail_account(
        &self,
        email: &str,
        username: &str,
        settings: crate::models::CustomMailSettings,
        password: &str,
    ) -> Result<Uuid> {
        let path = self
            .manager
            .create_mail_account(email, username, settings, password)
            .await?;
        let proxy = self.account_proxy(path).await?;
        Uuid::parse_str(&proxy.id().await?).map_err(|err| zbus::fdo::Error::Failed(err.to_string()))
    }

    pub async fn set_mail_password(&self, id: &Uuid, password: &str) -> Result<()> {
        self.account_proxy(account_path(id))
            .await?
            .set_mail_password(password)
            .await
    }

    pub async fn create_caldav_account(
        &self,
        url: &str,
        username: &str,
        password: &str,
    ) -> Result<Uuid> {
        let path = self
            .manager
            .create_caldav_account(url, username, password)
            .await?;
        let proxy = self.account_proxy(path).await?;
        Uuid::parse_str(&proxy.id().await?).map_err(|err| zbus::fdo::Error::Failed(err.to_string()))
    }

    pub async fn set_caldav_password(&self, id: &Uuid, password: &str) -> Result<()> {
        self.account_proxy(account_path(id))
            .await?
            .set_caldav_password(password)
            .await
    }

    pub async fn create_contacts_account(
        &self,
        settings: crate::models::ContactsConnectionSettings,
        username: &str,
        password: &str,
    ) -> Result<Uuid> {
        let path = self
            .manager
            .create_contacts_account(settings, username, password)
            .await?;
        let proxy = self.account_proxy(path).await?;
        Uuid::parse_str(&proxy.id().await?).map_err(|err| zbus::fdo::Error::Failed(err.to_string()))
    }

    pub async fn set_contacts_password(&self, id: &Uuid, password: &str) -> Result<()> {
        self.account_proxy(account_path(id))
            .await?
            .set_contacts_password(password)
            .await
    }

    /// Starts sign-in. Watch the request status and open `InteractionUri` when prompted.
    pub async fn create_account(&mut self, provider: &Provider) -> Result<RequestProxy<'static>> {
        let path = self
            .manager
            .create_account(provider, HashMap::new())
            .await?;
        self.request_proxy(path).await
    }

    pub async fn list_providers(&self) -> Result<Vec<DbusProviderInfo>> {
        let paths = self.manager.list_providers().await?;
        let mut providers = Vec::with_capacity(paths.len());
        for path in paths {
            let proxy = ProviderProxy::builder(&self.connection)
                .path(path)
                .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?
                .build()
                .await
                .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
            providers.push(DbusProviderInfo {
                id: proxy.id().await?,
                name: proxy.name().await?,
                services: proxy.services().await?,
                icon: {
                    let icon = proxy.icon_name().await?;
                    (!icon.is_empty()).then_some(icon)
                },
            });
        }
        Ok(providers)
    }

    pub async fn get_account(&self, id: &Uuid) -> Result<Account> {
        let proxy = self.account_proxy(account_path(id)).await?;
        Self::account_from_proxy(&proxy).await
    }

    pub async fn remove_account(&mut self, id: &Uuid) -> Result<()> {
        let proxy = self.account_proxy(account_path(id)).await?;
        proxy.remove().await
    }

    pub async fn set_account_enabled(&mut self, id: &Uuid, enabled: bool) -> Result<()> {
        let proxy = self.account_proxy(account_path(id)).await?;
        proxy.set_enabled(enabled).await
    }

    pub async fn set_service_enabled(
        &mut self,
        id: &Uuid,
        service: &Service,
        enabled: bool,
    ) -> Result<()> {
        let proxy = self.account_proxy(account_path(id)).await?;
        if enabled {
            proxy.enable_service(&service.to_string()).await
        } else {
            proxy.disable_service(&service.to_string()).await
        }
    }

    pub async fn ensure_credentials(&mut self, id: &Uuid) -> Result<()> {
        let proxy = self.account_proxy(account_path(id)).await?;
        proxy.ensure_credentials().await.map(|_| ())
    }

    /// Standing consent grants for this account: `(service, caller_identity, decision)`.
    pub async fn list_grants(&self, id: &Uuid) -> Result<Vec<(String, String, String)>> {
        let proxy = self.account_proxy(account_path(id)).await?;
        proxy.list_grants().await
    }

    pub async fn revoke_grant(
        &mut self,
        id: &Uuid,
        service: &str,
        caller_identity: &str,
    ) -> Result<()> {
        let proxy = self.account_proxy(account_path(id)).await?;
        proxy.revoke_grant(service, caller_identity).await
    }

    /// Returns `(access_token, expires_at)` after authorization for the requested service.
    /// The provider token may carry scopes for other services.
    pub async fn get_access_token(
        &mut self,
        id: &Uuid,
        service: &Service,
    ) -> Result<(String, i64)> {
        let proxy = CredentialsProxy::builder(&self.connection)
            .path(account_path(id))
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?
            .build()
            .await?;
        proxy.get_access_token(&service.to_string()).await
    }

    /// Starts reauthentication with the same request lifecycle as `create_account`.
    pub async fn reauthenticate(&mut self, id: &Uuid) -> Result<RequestProxy<'static>> {
        let proxy = self.account_proxy(account_path(id)).await?;
        let path = proxy.reauthenticate().await?;
        self.request_proxy(path).await
    }

    pub async fn receive_account_added(&self) -> zbus::Result<AccountAddedStream> {
        self.manager.receive_account_added().await
    }

    pub async fn receive_account_removed(&self) -> zbus::Result<AccountRemovedStream> {
        self.manager.receive_account_removed().await
    }

    pub async fn receive_account_toggled(&self) -> zbus::Result<AccountToggledStream> {
        self.manager.receive_account_toggled().await
    }

    /// Watches per-service enable/disable changes for an account.
    ///
    /// The signal is emitted from the account object with the new list of
    /// enabled services; callers can re-query the accounts they care about.
    pub async fn receive_services_changed(
        &self,
        id: &Uuid,
    ) -> zbus::Result<crate::proxy::ServicesChangedStream> {
        self.account_proxy(account_path(id))
            .await?
            .receive_services_changed()
            .await
    }
}
