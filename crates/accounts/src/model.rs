//! Public data model for the embedded accounts library.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

pub type AccountId = Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Service {
    #[serde(alias = "Email")]
    Mail,
    Calendar,
    Contacts,
    Tasks,
}

impl Service {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mail => "Mail",
            Self::Calendar => "Calendar",
            Self::Contacts => "Contacts",
            Self::Tasks => "Tasks",
        }
    }
}

impl fmt::Display for Service {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Provider {
    Google,
    Microsoft,
    #[serde(alias = "custom-caldav")]
    CustomCalDav,
    #[serde(alias = "custom-mail")]
    CustomImapSmtp,
    #[serde(alias = "custom-carddav")]
    CustomCardDav,
    #[serde(alias = "custom-ldap")]
    CustomLdap,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Microsoft => "microsoft",
            Self::CustomCalDav => "custom-caldav",
            Self::CustomImapSmtp => "custom-mail",
            Self::CustomCardDav => "custom-carddav",
            Self::CustomLdap => "custom-ldap",
        }
    }

    pub fn is_oauth(self) -> bool {
        matches!(self, Self::Google | Self::Microsoft)
    }
}

/// Identifies the resource a credential is valid for.
///
/// Microsoft Graph and Exchange Online are deliberately separate: a Graph
/// token cannot authenticate IMAP or SMTP, and an Exchange token cannot call
/// Graph. Keeping them separate also prevents scope-set collisions in the
/// credential cache.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Resource {
    Primary,
    Graph,
    Exchange,
    CalDav,
    CardDav,
    Ldap,
    ImapSmtp,
}

impl Resource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Graph => "graph",
            Self::Exchange => "exchange",
            Self::CalDav => "caldav",
            Self::CardDav => "carddav",
            Self::Ldap => "ldap",
            Self::ImapSmtp => "imap-smtp",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CalDavSettings {
    pub url: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImapSmtpSettings {
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_tls: bool,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_starttls: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LdapSettings {
    pub url: String,
    pub base_dn: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContactsConnectionSettings {
    pub protocol: String,
    pub uri: String,
    pub base_dn: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Endpoint {
    None,
    CalDav(CalDavSettings),
    ImapSmtp(ImapSmtpSettings),
    CardDav(CalDavSettings),
    Ldap(LdapSettings),
}

/// An account belongs to one app, one service and one identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub provider: Provider,
    pub service: Service,
    pub display_name: String,
    pub username: String,
    pub email: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    /// Incremented on reconnect and removal. Cached credentials and in-flight
    /// refreshes carrying an older generation are discarded.
    pub generation: u64,
    pub endpoint: Endpoint,
}

impl Account {
    pub fn email_or_identity(&self) -> &str {
        self.email
            .as_deref()
            .filter(|email| !email.is_empty())
            .unwrap_or(&self.username)
    }
}

/// A secret value that is cleared from memory on drop and redacted in logs.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(<redacted>)")
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Credential {
    pub account_id: AccountId,
    pub resource: Resource,
    pub access_token: String,
    pub refresh_token: Option<SecretString>,
    pub expires_at: Option<DateTime<Utc>>,
    pub scope: Vec<String>,
    pub token_type: String,
    /// Password or opaque handler blob for non-OAuth accounts.
    pub secret: Option<Vec<u8>>,
    pub generation: u64,
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credential")
            .field("resource", &self.resource)
            .field("scope", &self.scope)
            .field("token_type", &self.token_type)
            .field("expires_at", &self.expires_at)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl Drop for Credential {
    fn drop(&mut self) {
        self.access_token.zeroize();
        if let Some(secret) = &mut self.secret {
            secret.zeroize();
        }
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct StoredData {
    pub version: u32,
    pub accounts: Vec<Account>,
    pub credentials: Vec<Credential>,
}

pub const STORE_VERSION: u32 = 1;
