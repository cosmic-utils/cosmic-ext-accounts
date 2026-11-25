//! Embedded online account management for COSMIC applications.
//!
//! Each application owns its accounts and credentials. Enable only the
//! service features the application uses; the `libcosmic` feature adds an
//! embeddable settings panel.

pub mod config;
pub mod error;
pub mod manager;
pub mod model;
pub mod oauth;
pub mod provider;
pub mod service;
pub mod store;

#[cfg(feature = "libcosmic")]
pub mod ui;

pub use config::{ClientConfig, Config};
pub use error::{Error, Result};
pub use manager::{Accounts, Event};
pub use model::{
    Account, AccountId, CalDavSettings, ContactsConnectionSettings, Credential, Endpoint,
    ImapSmtpSettings, LdapSettings, Provider, Resource, SecretString, Service,
};
pub use oauth::{AuthRequest, CompletedAuth};

#[cfg(feature = "calendar")]
pub use service::CalendarCredentials;
#[cfg(feature = "contacts")]
pub use service::ContactsCredentials;
#[cfg(not(feature = "mail"))]
pub use service::MailConnectionSettings;
#[cfg(feature = "tasks")]
pub use service::TasksCredentials;
#[cfg(feature = "mail")]
pub use service::{MailConnectionSettings, MailCredentials, MailTransport};
