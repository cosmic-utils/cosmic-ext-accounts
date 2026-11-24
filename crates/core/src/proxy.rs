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

    async fn create_mail_account(
        &self,
        email: &str,
        username: &str,
        settings: crate::models::CustomMailSettings,
        password: &str,
    ) -> Result<OwnedObjectPath>;

    async fn create_caldav_account(
        &self,
        url: &str,
        username: &str,
        password: &str,
    ) -> Result<OwnedObjectPath>;

    async fn create_contacts_account(
        &self,
        settings: crate::models::ContactsConnectionSettings,
        username: &str,
        password: &str,
    ) -> Result<OwnedObjectPath>;

    #[zbus(property)]
    fn version(&self) -> Result<String>;

    /// Version of the mail endpoint published by this daemon. `0` means the
    /// daemon predates the mail endpoint and cannot serve mail consumers.
    #[zbus(property)]
    fn mail_endpoint_version(&self) -> Result<u32>;

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
    async fn get_custom_mail_settings(&self) -> Result<crate::models::CustomMailSettings>;

    async fn set_mail_password(&self, password: &str) -> Result<()>;

    async fn get_caldav_url(&self) -> Result<String>;
    async fn set_caldav_password(&self, password: &str) -> Result<()>;

    async fn get_custom_contacts_settings(
        &self,
    ) -> Result<crate::models::ContactsConnectionSettings>;
    async fn set_contacts_password(&self, password: &str) -> Result<()>;

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

    /// Returns validated settings, login name, and the custom mail password.
    async fn get_mail_password(&self) -> Result<(MailConnectionSettings, String, String)>;

    /// Returns the HTTPS server URL, username, and password for a CalDAV account.
    async fn get_caldav_access(&self) -> Result<(String, String, String)>;

    /// Returns the directory settings, username, and password after Contacts consent.
    async fn get_contact_access(
        &self,
    ) -> Result<(crate::models::ContactsConnectionSettings, String, String)>;

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
    async fn get_connection_settings(&self) -> Result<crate::models::ContactsConnectionSettings>;

    /// CardDAV collection/principal URL or the LDAPS server URL.
    #[zbus(property)]
    fn uri(&self) -> Result<String>;
    #[zbus(property)]
    fn auth_method(&self) -> Result<String>;
}

/// Authentication schemes advertised by a CalDAV endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarAuthMethod {
    Basic,
    OAuth2,
}

impl CalendarAuthMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::OAuth2 => "oauth2",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "basic" => Some(Self::Basic),
            "oauth2" => Some(Self::OAuth2),
            _ => None,
        }
    }
}

/// Authentication schemes advertised by a contacts endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactsAuthMethod {
    Basic,
    SimpleBind,
    OAuth2,
}

impl ContactsAuthMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::SimpleBind => "simple-bind",
            Self::OAuth2 => "oauth2",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "basic" => Some(Self::Basic),
            "simple-bind" => Some(Self::SimpleBind),
            "oauth2" => Some(Self::OAuth2),
            _ => None,
        }
    }
}

/// Authentication schemes advertised by a mail endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailAuthMethod {
    Password,
    OAuth2,
}

impl MailAuthMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::OAuth2 => "xoauth2",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "password" => Some(Self::Password),
            "xoauth2" => Some(Self::OAuth2),
            _ => None,
        }
    }
}

/// Mail transport settings in the order used by the D-Bus wire format.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize, zbus::zvariant::Type,
)]
pub struct MailConnectionSettings {
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_tls: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_tls: String,
    pub authentication: String,
}

impl MailConnectionSettings {
    pub fn auth_method(&self) -> Option<MailAuthMethod> {
        MailAuthMethod::parse(&self.authentication)
    }

    /// Whether these settings can be used to open an IMAP/SMTP session.
    pub fn is_supported(&self) -> bool {
        self.imap_tls == "tls"
            && matches!(self.smtp_tls.as_str(), "tls" | "starttls")
            && self.auth_method().is_some()
            && !self.imap_host.is_empty()
            && !self.smtp_host.is_empty()
            && self.imap_port != 0
            && self.smtp_port != 0
    }

    pub fn smtp_starttls(&self) -> bool {
        self.smtp_tls == "starttls"
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> MailConnectionSettings {
        MailConnectionSettings {
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            imap_tls: "tls".into(),
            smtp_host: "smtp.example.com".into(),
            smtp_port: 587,
            smtp_tls: "starttls".into(),
            authentication: "xoauth2".into(),
        }
    }

    #[test]
    fn mail_settings_are_validated_once_for_all_consumers() {
        let settings = settings();
        assert!(settings.is_supported());
        assert!(settings.smtp_starttls());
        assert_eq!(settings.auth_method(), Some(MailAuthMethod::OAuth2));

        let unsupported = MailConnectionSettings {
            imap_tls: "none".into(),
            ..settings
        };
        assert!(!unsupported.is_supported());
    }

    #[test]
    fn dav_auth_methods_are_parsed() {
        assert_eq!(
            CalendarAuthMethod::parse("basic"),
            Some(CalendarAuthMethod::Basic)
        );
        assert_eq!(
            CalendarAuthMethod::parse("oauth2"),
            Some(CalendarAuthMethod::OAuth2)
        );
        assert_eq!(CalendarAuthMethod::parse("password"), None);

        assert_eq!(
            ContactsAuthMethod::parse("simple-bind"),
            Some(ContactsAuthMethod::SimpleBind)
        );
        assert_eq!(
            ContactsAuthMethod::parse("oauth2"),
            Some(ContactsAuthMethod::OAuth2)
        );
        assert_eq!(ContactsAuthMethod::parse("xoauth2"), None);
    }
}
