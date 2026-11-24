pub mod clients;
pub mod config;
pub mod error;
pub mod models;
pub mod proxy;
pub mod registry;
mod service;
pub mod ui;

pub use clients::{
    AccountsClient, CalendarClient, CalendarCredentials, ContactsClient, ContactsCredentials,
    MailClient, MailCredentials, TasksClient, TasksCredentials,
};
pub use error::AccountsError;
pub use proxy::{CalendarAuthMethod, ContactsAuthMethod, MailAuthMethod, MailConnectionSettings};
pub use registry::{OAuthManifest, ProviderManifest, ProviderManifestInfo, ProviderRegistry};
pub use service::*;

pub use chrono::Local;
pub use uuid::Uuid;
pub use zbus;
