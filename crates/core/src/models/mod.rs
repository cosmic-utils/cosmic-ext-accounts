mod account;
mod credentials;
mod provider;
mod provider_info;
mod service;

pub use account::{Account, CUSTOM_MAIL_PROVIDER, CustomMailSettings, DbusAccount};
pub use credentials::Credential;
pub use provider::Provider;
pub use provider_info::{DbusProviderInfo, IconSource};
pub use service::{DbusService, Service};
