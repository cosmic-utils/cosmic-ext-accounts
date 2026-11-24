pub mod clients;
pub mod config;
pub mod error;
pub mod models;
pub mod proxy;
pub mod registry;
pub mod ui;
mod service;

pub use clients::{AccountsClient, MailClient, MailCredentials};
pub use error::AccountsError;
pub use proxy::{MailAuthMethod, MailConnectionSettings};
pub use registry::{OAuthManifest, ProviderManifest, ProviderManifestInfo, ProviderRegistry};
pub use service::*;

pub use chrono::Local;
pub use uuid::Uuid;
pub use zbus;
