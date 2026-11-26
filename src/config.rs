//! Application-supplied configuration.

use std::{collections::HashMap, path::PathBuf};

use crate::{
    error::{Error, Result},
    model::{Provider, Resource, SecretString, Service},
    provider::spec,
};

/// OAuth client registration for one provider and service.
#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub client_id: String,
    pub client_secret: Option<SecretString>,
    pub redirect_uri: Option<String>,
}

impl ClientConfig {
    /// Reads a client registration from environment variables.
    pub fn from_env(prefix: &str) -> Option<Self> {
        let client_id = std::env::var(format!("{prefix}_CLIENT_ID")).ok()?;
        let client_secret = std::env::var(format!("{prefix}_CLIENT_SECRET"))
            .ok()
            .map(SecretString::new);
        let redirect_uri = std::env::var(format!("{prefix}_REDIRECT_URI")).ok();
        Some(Self {
            client_id,
            client_secret,
            redirect_uri,
        })
    }
}

pub type ClientKey = (Provider, Service, Option<Resource>);

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub app_id: String,
    pub clients: HashMap<ClientKey, ClientConfig>,
    pub data_dir: Option<PathBuf>,
}

impl Config {
    pub fn new(app_id: impl Into<String>) -> Self {
        Self {
            app_id: app_id.into(),
            ..Self::default()
        }
    }

    pub fn with_client(
        mut self,
        provider: Provider,
        service: Service,
        client: ClientConfig,
    ) -> Self {
        self.clients.insert((provider, service, None), client);
        self
    }

    pub fn with_resource_client(
        mut self,
        provider: Provider,
        service: Service,
        resource: Resource,
        client: ClientConfig,
    ) -> Self {
        self.clients
            .insert((provider, service, Some(resource)), client);
        self
    }

    /// Adds a service client from `ACCOUNTS_<PROVIDER>_<SERVICE>_*`.
    pub fn with_client_from_env(self, provider: Provider, service: Service) -> Self {
        if let Some(client) = ClientConfig::from_env(&env_prefix(provider, service, None)) {
            self.with_client(provider, service, client)
        } else {
            self
        }
    }

    /// Adds a resource-specific client from
    /// `ACCOUNTS_<PROVIDER>_<SERVICE>_<RESOURCE>_*`.
    pub fn with_resource_client_from_env(
        self,
        provider: Provider,
        service: Service,
        resource: Resource,
    ) -> Self {
        if let Some(client) = ClientConfig::from_env(&env_prefix(provider, service, Some(resource)))
        {
            self.with_resource_client(provider, service, resource, client)
        } else {
            self
        }
    }

    pub fn client(
        &self,
        provider: Provider,
        service: Service,
        resource: Resource,
    ) -> Result<(ClientConfig, String)> {
        let config = self
            .clients
            .get(&(provider, service, Some(resource)))
            .or_else(|| self.clients.get(&(provider, service, None)))
            .ok_or(Error::MissingClientConfig(provider, service))?;

        if config.client_id.trim().is_empty() {
            return Err(Error::InvalidClientConfig(
                "client_id must not be empty".into(),
            ));
        }

        let redirect = config.redirect_uri.as_deref().unwrap_or_else(|| {
            spec(provider)
                .map(|spec| spec.default_redirect_uri)
                .unwrap_or("http://127.0.0.1/callback")
        });
        validate_loopback_redirect(redirect)?;
        Ok((config.clone(), redirect.to_owned()))
    }

    pub fn data_dir(&self) -> Result<PathBuf> {
        if let Some(dir) = &self.data_dir {
            return Ok(dir.clone());
        }
        self.default_data_dir()
    }

    pub(crate) fn default_data_dir(&self) -> Result<PathBuf> {
        let base = directories::BaseDirs::new().ok_or_else(|| {
            Error::StorageUnavailable("could not resolve user data directory".into())
        })?;
        Ok(base.data_dir().join(&self.app_id))
    }
}

fn env_prefix(provider: Provider, service: Service, resource: Option<Resource>) -> String {
    let provider = match provider {
        Provider::Google => "GOOGLE",
        Provider::Microsoft => "MICROSOFT",
        Provider::CustomCalDav => "CUSTOM_CALDAV",
        Provider::CustomImapSmtp => "CUSTOM_MAIL",
        Provider::CustomCardDav => "CUSTOM_CARDDAV",
        Provider::CustomLdap => "CUSTOM_LDAP",
    };
    let service = service.as_str().to_uppercase();
    match resource {
        Some(resource) => format!(
            "ACCOUNTS_{provider}_{service}_{}",
            resource.as_str().to_uppercase().replace('-', "_")
        ),
        None => format!("ACCOUNTS_{provider}_{service}"),
    }
}

fn validate_loopback_redirect(uri: &str) -> Result<()> {
    let parsed = url::Url::parse(uri)
        .map_err(|_| Error::InvalidClientConfig("redirect URI is not a valid URL".into()))?;
    if parsed.scheme() != "http" || parsed.host_str() != Some("127.0.0.1") {
        return Err(Error::InvalidClientConfig(
            "redirect URI must use http://127.0.0.1".into(),
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() || parsed.fragment().is_some() {
        return Err(Error::InvalidClientConfig(
            "redirect URI must not contain credentials or a fragment".into(),
        ));
    }
    Ok(())
}
