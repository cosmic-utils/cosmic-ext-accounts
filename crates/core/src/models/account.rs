use std::{collections::BTreeMap, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zbus::zvariant::{DeserializeDict, SerializeDict, Type};

use crate::models::{Provider, Service};

pub const CUSTOM_CARDDAV_PROVIDER: &str = "custom-carddav";
pub const CUSTOM_LDAP_PROVIDER: &str = "custom-ldap";
pub const CUSTOM_CALDAV_PROVIDER: &str = "custom-caldav";
pub const CUSTOM_MAIL_PROVIDER: &str = "custom-mail";

/// Field-level validation failure for a custom mail account form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustomMailSettingsError {
    ImapHost,
    ImapPort,
    SmtpHost,
    SmtpPort,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Type)]
pub struct CustomMailSettings {
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_starttls: bool,
}

impl CustomMailSettings {
    /// Validates host names and ports shared by every custom-mail form.
    pub fn validate(&self) -> Result<(), CustomMailSettingsError> {
        if self.imap_host.trim().is_empty() {
            return Err(CustomMailSettingsError::ImapHost);
        }
        if self.imap_port == 0 {
            return Err(CustomMailSettingsError::ImapPort);
        }
        if self.smtp_host.trim().is_empty() {
            return Err(CustomMailSettingsError::SmtpHost);
        }
        if self.smtp_port == 0 {
            return Err(CustomMailSettingsError::SmtpPort);
        }
        Ok(())
    }
}

/// Field-level validation failure for a custom contacts account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactsConnectionSettingsError {
    UnsupportedProtocol,
    InvalidUri,
    InsecureUri,
    BaseDnRequired,
    UnexpectedBaseDn,
}

/// Directory settings. The base DN is empty for CardDAV.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Type)]
pub struct ContactsConnectionSettings {
    pub protocol: String,
    pub uri: String,
    pub base_dn: String,
}

impl ContactsConnectionSettings {
    /// Validates the protocol, secure URI and base DN expected by each backend.
    pub fn validate(&self) -> Result<(), ContactsConnectionSettingsError> {
        let expected_scheme = match self.protocol.as_str() {
            "carddav" => "https://",
            "ldap" => "ldaps://",
            _ => return Err(ContactsConnectionSettingsError::UnsupportedProtocol),
        };

        if self.uri.is_empty()
            || self
                .uri
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(ContactsConnectionSettingsError::InvalidUri);
        }
        if !self.uri.starts_with(expected_scheme) {
            return Err(ContactsConnectionSettingsError::InsecureUri);
        }

        match self.protocol.as_str() {
            "carddav" if !self.base_dn.is_empty() => {
                Err(ContactsConnectionSettingsError::UnexpectedBaseDn)
            }
            "ldap" if self.base_dn.trim().is_empty() => {
                Err(ContactsConnectionSettingsError::BaseDnRequired)
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Account {
    pub id: Uuid,
    pub provider: Provider,
    pub display_name: String,
    pub username: String,
    pub email: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
    pub services: BTreeMap<Service, bool>,
    #[serde(default)]
    pub custom_mail: Option<CustomMailSettings>,
    #[serde(default)]
    pub caldav_url: Option<String>,
    #[serde(default)]
    pub custom_contacts: Option<ContactsConnectionSettings>,
}

impl Account {
    pub fn dbus_id(&self) -> String {
        self.id.to_string().replace("-", "_")
    }

    /// Whether this account is enabled and offers the given service.
    pub fn is_enabled_for(&self, service: &Service) -> bool {
        self.enabled && self.services.get(service).copied().unwrap_or(false)
    }

    /// Best label for the account when it has no email address.
    pub fn email_or_identity(&self) -> &str {
        self.email
            .as_deref()
            .filter(|email| !email.is_empty())
            .unwrap_or(&self.username)
    }
}

#[derive(Debug, Clone, PartialEq, DeserializeDict, SerializeDict, Type)]
#[zvariant(signature = "dict")]
pub struct DbusAccount {
    pub id: String,
    pub provider: String,
    pub display_name: String,
    pub username: String,
    pub email: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub last_used: Option<String>,
    pub services: BTreeMap<String, bool>,
}

impl From<Account> for DbusAccount {
    fn from(value: Account) -> Self {
        Self {
            id: value.id.to_string(),
            provider: value.provider,
            display_name: value.display_name,
            username: value.username,
            email: value.email,
            enabled: value.enabled,
            created_at: value.created_at.to_string(),
            last_used: value
                .last_used
                .clone()
                .map(|last_used| last_used.to_string()),
            services: value
                .services
                .iter()
                .map(|(service, enabled)| (service.to_string(), *enabled))
                .collect(),
        }
    }
}

impl From<&Account> for DbusAccount {
    fn from(value: &Account) -> Self {
        Self {
            id: value.id.to_string(),
            provider: value.provider.clone(),
            display_name: value.display_name.clone(),
            username: value.username.clone(),
            email: value.email.clone(),
            enabled: value.enabled,
            created_at: value.created_at.to_string(),
            last_used: value
                .last_used
                .clone()
                .map(|last_used| last_used.to_string()),
            services: value
                .services
                .iter()
                .map(|(service, enabled)| (service.to_string(), *enabled))
                .collect(),
        }
    }
}

impl From<DbusAccount> for Account {
    fn from(value: DbusAccount) -> Self {
        Account {
            id: Uuid::from_str(&value.id).unwrap(),
            provider: value.provider,
            display_name: value.display_name,
            username: value.username,
            email: value.email,
            enabled: value.enabled,
            created_at: DateTime::from_str(&value.created_at).unwrap(),
            last_used: value
                .last_used
                .map(|lu| DateTime::from_str(&lu).ok())
                .unwrap(),
            custom_mail: None,
            caldav_url: None,
            custom_contacts: None,
            services: value
                .services
                .into_iter()
                .map(|(service, enabled)| (Service::from_str(service).unwrap(), enabled))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_mail_settings_validate_hosts_and_ports() {
        let valid = CustomMailSettings {
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            smtp_host: "smtp.example.com".into(),
            smtp_port: 587,
            smtp_starttls: true,
        };
        assert_eq!(valid.validate(), Ok(()));

        let invalid = CustomMailSettings {
            imap_port: 0,
            ..valid
        };
        assert_eq!(invalid.validate(), Err(CustomMailSettingsError::ImapPort));
    }

    #[test]
    fn contacts_settings_validate_protocol_uri_and_base_dn() {
        let carddav = ContactsConnectionSettings {
            protocol: "carddav".into(),
            uri: "https://dav.example.com".into(),
            base_dn: String::new(),
        };
        assert_eq!(carddav.validate(), Ok(()));

        let ldap = ContactsConnectionSettings {
            protocol: "ldap".into(),
            uri: "ldaps://ldap.example.com".into(),
            base_dn: "dc=example,dc=com".into(),
        };
        assert_eq!(ldap.validate(), Ok(()));

        let insecure = ContactsConnectionSettings {
            uri: "http://dav.example.com".into(),
            ..carddav
        };
        assert_eq!(
            insecure.validate(),
            Err(ContactsConnectionSettingsError::InsecureUri)
        );

        let missing_base_dn = ContactsConnectionSettings {
            base_dn: String::new(),
            ..ldap
        };
        assert_eq!(
            missing_base_dn.validate(),
            Err(ContactsConnectionSettingsError::BaseDnRequired)
        );
    }
}
