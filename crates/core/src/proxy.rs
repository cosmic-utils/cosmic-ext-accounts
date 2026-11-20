use std::collections::HashMap;

use zbus::fdo::Result;
use zbus::proxy;
use zbus::zvariant::OwnedObjectPath;

#[proxy(
    default_service = "dev.edfloreshz.Accounts",
    default_path = "/dev/edfloreshz/Accounts/Manager",
    interface = "dev.edfloreshz.Accounts.Manager"
)]
pub trait Manager {
    async fn list_accounts(&self) -> Result<Vec<OwnedObjectPath>>;
    async fn list_providers(&self) -> Result<Vec<OwnedObjectPath>>;
    async fn create_account(
        &self,
        provider_id: &str,
        params: HashMap<String, String>,
    ) -> Result<OwnedObjectPath>;

    #[zbus(property)]
    fn version(&self) -> Result<String>;

    #[zbus(signal)]
    fn account_added(account: OwnedObjectPath) -> Result<()>;

    #[zbus(signal)]
    fn account_removed(account: OwnedObjectPath) -> Result<()>;

    /// Reports the account path and its new enabled state.
    #[zbus(signal)]
    fn account_toggled(account: OwnedObjectPath, enabled: bool) -> Result<()>;
}

#[proxy(
    default_service = "dev.edfloreshz.Accounts",
    interface = "dev.edfloreshz.Accounts.Request"
)]
pub trait Request {
    #[zbus(property)]
    fn status(&self) -> Result<String>;

    #[zbus(property)]
    fn interaction_uri(&self) -> Result<String>;

    #[zbus(property)]
    fn error_message(&self) -> Result<String>;

    #[zbus(property)]
    fn account(&self) -> Result<OwnedObjectPath>;

    async fn cancel(&self) -> Result<()>;

    #[zbus(signal, name = "StatusChanged")]
    fn request_status_changed(status: &str) -> Result<()>;
}

#[proxy(
    default_service = "dev.edfloreshz.Accounts",
    interface = "dev.edfloreshz.Accounts.Account"
)]
pub trait Account {
    #[zbus(property)]
    fn id(&self) -> Result<String>;

    #[zbus(property)]
    fn provider_id(&self) -> Result<String>;

    #[zbus(property)]
    fn display_name(&self) -> Result<String>;

    #[zbus(property)]
    fn set_display_name(&self, value: &str) -> Result<()>;

    #[zbus(property)]
    fn identity(&self) -> Result<String>;

    #[zbus(property)]
    fn enabled(&self) -> Result<bool>;

    #[zbus(property)]
    fn set_enabled(&self, value: bool) -> Result<()>;

    #[zbus(property)]
    fn available_services(&self) -> Result<Vec<String>>;

    #[zbus(property)]
    fn enabled_services(&self) -> Result<Vec<String>>;

    #[zbus(property)]
    fn created_at(&self) -> Result<String>;

    #[zbus(property)]
    fn last_used(&self) -> Result<String>;

    #[zbus(property)]
    fn email(&self) -> Result<String>;

    async fn enable_service(&self, service: &str) -> Result<()>;
    async fn disable_service(&self, service: &str) -> Result<()>;
    async fn remove(&self) -> Result<()>;
    async fn ensure_credentials(&self) -> Result<()>;
    async fn reauthenticate(&self) -> Result<OwnedObjectPath>;

    /// `a(sss)` of `(service, caller_identity, decision)`.
    async fn list_grants(&self) -> Result<Vec<(String, String, String)>>;
    async fn revoke_grant(&self, service: &str, caller_identity: &str) -> Result<()>;

    #[zbus(signal)]
    fn services_changed(enabled_services: Vec<String>) -> Result<()>;
}

/// Credential interface on the account's object path.
#[proxy(
    default_service = "dev.edfloreshz.Accounts",
    interface = "dev.edfloreshz.Accounts.Credentials"
)]
pub trait Credentials {
    #[zbus(property)]
    fn auth_method(&self) -> Result<String>;

    /// Returns the access token and expiry in Unix seconds (0 if unavailable).
    async fn get_access_token(&self, service: &str) -> Result<(String, i64)>;

    /// Returns the validated settings, access token, and expiry for IMAP/SMTP.
    async fn get_mail_access(&self) -> Result<(MailConnectionSettings, String, i64)>;

    async fn invalidate_token(&self) -> Result<()>;
}

#[proxy(
    default_service = "dev.edfloreshz.Accounts",
    interface = "dev.edfloreshz.Accounts.Provider"
)]
pub trait Provider {
    #[zbus(property)]
    fn id(&self) -> Result<String>;

    #[zbus(property)]
    fn name(&self) -> Result<String>;

    #[zbus(property)]
    fn icon_name(&self) -> Result<String>;

    #[zbus(property)]
    fn services(&self) -> Result<Vec<String>>;

    #[zbus(property)]
    fn auth_method(&self) -> Result<String>;
}

#[proxy(
    interface = "dev.edfloreshz.Accounts.Endpoint.Calendar",
    default_service = "dev.edfloreshz.Accounts"
)]
pub trait Calendar {
    /// CalDAV collection/principal URL.
    #[zbus(property)]
    fn uri(&self) -> Result<String>;
    #[zbus(property)]
    fn auth_method(&self) -> Result<String>;
}

#[proxy(
    interface = "dev.edfloreshz.Accounts.Endpoint.Tasks",
    default_service = "dev.edfloreshz.Accounts"
)]
pub trait Tasks {
    /// CalDAV collection URL for VTODO components; may equal the calendar URL.
    #[zbus(property)]
    fn uri(&self) -> Result<String>;
    #[zbus(property)]
    fn auth_method(&self) -> Result<String>;
}

#[proxy(
    interface = "dev.edfloreshz.Accounts.Endpoint.Contacts",
    default_service = "dev.edfloreshz.Accounts"
)]
pub trait Contacts {
    /// CardDAV collection/principal URL.
    #[zbus(property)]
    fn uri(&self) -> Result<String>;
    #[zbus(property)]
    fn auth_method(&self) -> Result<String>;
}

/// Mail transport settings in the order used by the D-Bus wire format.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, zbus::zvariant::Type)]
pub struct MailConnectionSettings {
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_tls: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_tls: String,
    pub authentication: String,
}

#[proxy(
    interface = "dev.edfloreshz.Accounts.Endpoint.Mail",
    default_service = "dev.edfloreshz.Accounts"
)]
pub trait Mail {
    async fn get_connection_settings(&self) -> Result<MailConnectionSettings>;

    #[zbus(property)]
    fn imap_host(&self) -> Result<String>;
    #[zbus(property)]
    fn imap_port(&self) -> Result<u16>;
    #[zbus(property)]
    fn smtp_host(&self) -> Result<String>;
    #[zbus(property)]
    fn smtp_port(&self) -> Result<u16>;
    #[zbus(property)]
    fn auth_method(&self) -> Result<String>;
}

pub const HANDLER_BLOB_PARAM: &str = "dev.edfloreshz.Accounts.credential_blob";

#[proxy(interface = "dev.edfloreshz.Accounts.ProviderHandler")]
pub trait ProviderHandler {
    /// Returns the account identity and an opaque credential blob.
    async fn authenticate(&self, params: HashMap<String, String>) -> Result<(String, Vec<u8>)>;
}
