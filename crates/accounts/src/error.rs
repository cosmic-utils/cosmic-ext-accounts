//! Errors returned by the embedded accounts library.

use thiserror::Error;

use crate::model::{Provider, Service};

#[derive(Debug, Error)]
pub enum Error {
    #[error("the {0} feature is not enabled in this build")]
    FeatureDisabled(&'static str),

    #[error("provider {0:?} does not support {1}")]
    UnsupportedProvider(Provider, Service),

    #[error("no client configuration for {0:?}/{1}")]
    MissingClientConfig(Provider, Service),

    #[error("invalid client configuration: {0}")]
    InvalidClientConfig(String),

    #[error("secure storage is unavailable: {0}")]
    StorageUnavailable(String),

    #[error("the credential store could not be read: {0}")]
    StorageCorrupt(String),

    #[error("account {0} was not found")]
    AccountNotFound(String),

    #[error("an account for {0:?}/{1} already exists")]
    AccountAlreadyExists(Provider, Service),

    #[error("credentials for account {0} are missing or stale; reconnect the account")]
    NeedsReauth(String),

    #[error("authentication was cancelled")]
    Cancelled,

    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("invalid OAuth callback state")]
    InvalidState,

    #[error("the OAuth callback timed out")]
    CallbackTimeout,

    #[error("invalid endpoint settings: {0}")]
    InvalidEndpoint(String),

    #[error("network request failed: {0}")]
    Network(String),

    #[error("unexpected provider response: {0}")]
    ProviderResponse(String),
}

pub type Result<T> = std::result::Result<T, Error>;
