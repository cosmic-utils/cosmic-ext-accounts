//! Account lifecycle, credential issuance and change notifications.

use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use oauth2::{
    AuthUrl, ClientId, ClientSecret, RefreshToken, Scope, TokenResponse, TokenUrl,
    basic::BasicClient, reqwest::async_http_client,
};
use tokio::sync::{Mutex, broadcast};
#[cfg(any(feature = "calendar", feature = "mail", feature = "contacts"))]
use uuid::Uuid;

#[cfg(all(feature = "calendar", not(feature = "contacts")))]
use crate::model::CalDavSettings;
#[cfg(any(feature = "calendar", feature = "mail", feature = "contacts"))]
use crate::model::Endpoint;
#[cfg(feature = "mail")]
use crate::model::ImapSmtpSettings;
#[cfg(feature = "contacts")]
use crate::model::{CalDavSettings, LdapSettings};
use crate::{
    config::Config,
    error::{Error, Result},
    model::{
        Account, AccountId, Credential, Provider, Resource, SecretString, Service, StoredData,
    },
    oauth::{AuthManager, AuthRequest},
    provider::{self, spec},
    store::SecretStore,
};

#[derive(Clone, Debug)]
pub enum Event {
    AccountsChanged(Service),
}

#[derive(Clone, Eq, PartialEq, Hash)]
struct CacheKey {
    account_id: AccountId,
    resource: Resource,
    scope: Vec<String>,
}

struct CachedToken {
    token: String,
    expires_at: DateTime<Utc>,
    generation: u64,
}

pub struct Accounts {
    config: Config,
    store: SecretStore,
    data: Mutex<StoredData>,
    auth: Mutex<AuthManager>,
    reconnects: Mutex<HashMap<String, AccountId>>,
    refresh_locks: Mutex<HashMap<AccountId, Arc<Mutex<()>>>>,
    cache: Mutex<HashMap<CacheKey, CachedToken>>,
    events: broadcast::Sender<Event>,
}

impl Accounts {
    /// Opens an app's credential store. This fails if secure storage is
    /// unavailable or the existing store cannot be decrypted.
    pub async fn new(config: Config) -> Result<Self> {
        let store = SecretStore::open(&config).await?;
        let data = store.load()?;
        let auth = AuthManager::new(config.clone());
        let (events, _) = broadcast::channel(16);
        Ok(Self {
            config,
            store,
            data: Mutex::new(data),
            auth: Mutex::new(auth),
            reconnects: Mutex::new(HashMap::new()),
            refresh_locks: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
            events,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    fn emit(&self, service: Service) {
        let _ = self.events.send(Event::AccountsChanged(service));
    }

    pub fn providers(&self, service: Service) -> Vec<Provider> {
        provider::providers_for(service)
    }

    pub async fn begin_auth(&self, provider: Provider, service: Service) -> Result<AuthRequest> {
        self.auth.lock().await.begin(provider, service).await
    }

    pub async fn cancel_auth(&self, state: &str) {
        self.auth.lock().await.cancel(state);
        self.reconnects.lock().await.remove(state);
    }

    /// Completes a sign-in and persists the account and its credentials.
    pub async fn complete_auth(&self, request: AuthRequest) -> Result<Account> {
        let state = request.state.clone();
        let service = request.service;
        let reconnect_id = self.reconnects.lock().await.remove(&state);

        let pending = self
            .auth
            .lock()
            .await
            .take(&state)
            .ok_or(Error::InvalidState)?;
        let completed = AuthManager::complete(pending.0, pending.1, request).await?;
        let account_guard = self.lock_for(completed.account.id).await;
        let _guard = account_guard.lock().await;
        let mut data = self.data.lock().await;
        let mut next = data.clone();

        let account = match reconnect_id {
            Some(id) => {
                let mut existing = next
                    .accounts
                    .iter()
                    .find(|account| account.id == id)
                    .cloned()
                    .ok_or_else(|| Error::AccountNotFound(id.to_string()))?;
                if existing.username != completed.account.username
                    || existing.email != completed.account.email
                {
                    return Err(Error::AuthenticationFailed(
                        "sign in with the same account to reconnect it".into(),
                    ));
                }
                existing.generation += 1;
                existing.display_name = completed.account.display_name;
                existing.endpoint = completed.account.endpoint;
                existing.enabled = true;
                existing
            }
            None => {
                if next.accounts.iter().any(|account| {
                    account.provider == completed.account.provider
                        && account.service == completed.account.service
                        && account.username == completed.account.username
                }) {
                    return Err(Error::AccountAlreadyExists(
                        completed.account.provider,
                        completed.account.service,
                    ));
                }
                completed.account
            }
        };

        let id = account.id;
        let generation = account.generation;
        next.credentials
            .retain(|credential| credential.account_id != id);
        next.credentials
            .extend(completed.credentials.into_iter().map(|mut credential| {
                credential.account_id = id;
                credential.generation = generation;
                credential
            }));
        if let Some(existing) = next.accounts.iter_mut().find(|item| item.id == id) {
            *existing = account.clone();
        } else {
            next.accounts.push(account.clone());
        }
        self.store.save(&next)?;
        *data = next;
        drop(data);
        drop(_guard);

        self.cache
            .lock()
            .await
            .retain(|key, _| key.account_id != id);
        self.emit(service);
        Ok(account)
    }

    /// Starts a reconnect for an existing account.
    pub async fn reconnect(&self, id: AccountId) -> Result<AuthRequest> {
        let account = self
            .get(id)
            .await
            .ok_or_else(|| Error::AccountNotFound(id.to_string()))?;
        let request = self.begin_auth(account.provider, account.service).await?;
        self.reconnects
            .lock()
            .await
            .insert(request.state.clone(), id);
        Ok(request)
    }

    pub async fn list(&self, service: Service) -> Vec<Account> {
        self.data
            .lock()
            .await
            .accounts
            .iter()
            .filter(|account| account.service == service)
            .cloned()
            .collect()
    }

    pub async fn get(&self, id: AccountId) -> Option<Account> {
        self.data
            .lock()
            .await
            .accounts
            .iter()
            .find(|account| account.id == id)
            .cloned()
    }

    pub async fn set_enabled(&self, id: AccountId, enabled: bool) -> Result<()> {
        let guard = self.lock_for(id).await;
        let _reconnect_guard = guard.lock().await;
        let mut data = self.data.lock().await;
        let mut next = data.clone();
        let account = next
            .accounts
            .iter_mut()
            .find(|account| account.id == id)
            .ok_or_else(|| Error::AccountNotFound(id.to_string()))?;
        account.enabled = enabled;
        let service = account.service;
        self.store.save(&next)?;
        *data = next;
        drop(data);
        drop(_reconnect_guard);
        self.emit(service);
        Ok(())
    }

    /// Removes one app/service account. Accounts belonging to other apps or
    /// services live in their own encrypted stores and are unaffected.
    pub async fn remove(&self, id: AccountId) -> Result<()> {
        let guard = self.lock_for(id).await;
        let _account_guard = guard.lock().await;
        let mut data = self.data.lock().await;
        let account = data
            .accounts
            .iter()
            .find(|account| account.id == id)
            .cloned()
            .ok_or_else(|| Error::AccountNotFound(id.to_string()))?;
        let mut next = data.clone();
        next.accounts.retain(|account| account.id != id);
        next.credentials
            .retain(|credential| credential.account_id != id);
        self.store.save(&next)?;
        *data = next;
        drop(data);
        drop(_account_guard);

        self.refresh_locks.lock().await.remove(&id);
        self.cache
            .lock()
            .await
            .retain(|key, _| key.account_id != id);
        self.emit(account.service);
        Ok(())
    }

    /// Discards the cached token for a resource and fetches a fresh one.
    pub async fn refresh(&self, id: AccountId, resource: Resource) -> Result<Credential> {
        let account = self
            .get(id)
            .await
            .ok_or_else(|| Error::AccountNotFound(id.to_string()))?;
        let scope = provider::resource_scopes(account.provider, account.service, resource);
        self.cache.lock().await.remove(&CacheKey {
            account_id: id,
            resource,
            scope,
        });
        self.credential(id, resource).await
    }

    /// Returns a token for the service's primary resource.
    pub async fn access_token(
        &self,
        id: AccountId,
        service: Service,
        force_refresh: bool,
    ) -> Result<String> {
        let account = self
            .get(id)
            .await
            .ok_or_else(|| Error::AccountNotFound(id.to_string()))?;
        let resource = provider::primary_resource(account.provider, service);
        if force_refresh {
            let scope = provider::resource_scopes(account.provider, service, resource);
            self.cache.lock().await.remove(&CacheKey {
                account_id: id,
                resource,
                scope,
            });
        }
        Ok(self.credential(id, resource).await?.access_token.clone())
    }

    /// Returns credentials for a resource, refreshing transparently when the
    /// cached token is missing or close to expiry.
    pub async fn credential(&self, id: AccountId, resource: Resource) -> Result<Credential> {
        let account = self
            .get(id)
            .await
            .ok_or_else(|| Error::AccountNotFound(id.to_string()))?;
        let scope = provider::resource_scopes(account.provider, account.service, resource);
        let key = CacheKey {
            account_id: id,
            resource,
            scope: scope.clone(),
        };

        if let Some(cached) = self.cache.lock().await.get(&key)
            && cached.generation == account.generation
            && cached.expires_at > Utc::now() + ChronoDuration::seconds(30)
        {
            let mut credential = self
                .stored_credential(id, resource, account.generation)
                .await?;
            credential.access_token = cached.token.clone();
            return Ok(credential);
        }

        let guard = self.lock_for(id).await;
        let _refresh_guard = guard.lock().await;
        let account = self
            .get(id)
            .await
            .ok_or_else(|| Error::AccountNotFound(id.to_string()))?;
        if let Some(cached) = self.cache.lock().await.get(&key)
            && cached.generation == account.generation
            && cached.expires_at > Utc::now() + ChronoDuration::seconds(30)
        {
            let mut credential = self
                .stored_credential(id, resource, account.generation)
                .await?;
            credential.access_token = cached.token.clone();
            return Ok(credential);
        }

        let stored = self
            .stored_credential(id, resource, account.generation)
            .await?;
        if stored.token_type == "password" {
            return Ok(stored);
        }
        let refresh_token = stored
            .refresh_token
            .clone()
            .ok_or_else(|| Error::NeedsReauth(id.to_string()))?;

        let (client_config, _) = self
            .config
            .client(account.provider, account.service, resource)?;
        let oauth = spec(account.provider).and_then(|spec| spec.oauth).ok_or(
            Error::UnsupportedProvider(account.provider, account.service),
        )?;
        let client = BasicClient::new(
            ClientId::new(client_config.client_id),
            client_config
                .client_secret
                .map(|secret| ClientSecret::new(secret.expose().to_owned())),
            AuthUrl::new(oauth.auth_url.to_owned())
                .map_err(|error| Error::InvalidClientConfig(error.to_string()))?,
            Some(
                TokenUrl::new(oauth.token_url.to_owned())
                    .map_err(|error| Error::InvalidClientConfig(error.to_string()))?,
            ),
        );

        let refresh = RefreshToken::new(refresh_token.expose().to_owned());
        let mut request = client.exchange_refresh_token(&refresh);
        for item in &scope {
            request = request.add_scope(Scope::new(item.clone()));
        }
        let response = request
            .request_async(async_http_client)
            .await
            .map_err(|error| Error::Network(error.to_string()))?;
        let access_token = response.access_token().secret().clone();
        let expires_at = response
            .expires_in()
            .map(|duration| Utc::now() + ChronoDuration::seconds(duration.as_secs() as i64));
        let next_refresh = response
            .refresh_token()
            .map(|token| SecretString::new(token.secret().clone()))
            .unwrap_or(refresh_token);

        let mut data = self.data.lock().await;
        let generation = data
            .accounts
            .iter()
            .find(|account| account.id == id)
            .ok_or_else(|| Error::AccountNotFound(id.to_string()))?
            .generation;
        if generation != account.generation {
            return Err(Error::NeedsReauth(id.to_string()));
        }
        let updated = Credential {
            account_id: id,
            resource,
            access_token: access_token.clone(),
            refresh_token: Some(next_refresh),
            expires_at,
            scope: scope.clone(),
            token_type: "Bearer".into(),
            secret: None,
            generation,
        };
        let mut next = data.clone();
        next.credentials
            .retain(|credential| credential.account_id != id || credential.resource != resource);
        next.credentials.push(updated.clone());
        self.store.save(&next)?;
        *data = next;
        drop(data);
        drop(_refresh_guard);

        if let Some(expires_at) = expires_at {
            self.cache.lock().await.insert(
                key,
                CachedToken {
                    token: access_token,
                    expires_at,
                    generation: account.generation,
                },
            );
        }
        Ok(updated)
    }

    async fn stored_credential(
        &self,
        id: AccountId,
        resource: Resource,
        generation: u64,
    ) -> Result<Credential> {
        let data = self.data.lock().await;
        data.credentials
            .iter()
            .find(|credential| {
                credential.account_id == id
                    && credential.resource == resource
                    && credential.generation == generation
            })
            .cloned()
            .ok_or_else(|| Error::NeedsReauth(id.to_string()))
    }

    async fn lock_for(&self, id: AccountId) -> Arc<Mutex<()>> {
        let mut locks = self.refresh_locks.lock().await;
        locks
            .entry(id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    #[cfg(any(feature = "calendar", feature = "mail", feature = "contacts"))]
    async fn insert_password_account(
        &self,
        provider: Provider,
        service: Service,
        username: String,
        email: Option<String>,
        endpoint: Endpoint,
        password: SecretString,
    ) -> Result<Account> {
        provider::ensure_supported(provider, service)?;
        let account = Account {
            id: Uuid::new_v4(),
            provider,
            service,
            display_name: username.clone(),
            username,
            email,
            enabled: true,
            created_at: Utc::now(),
            generation: 1,
            endpoint,
        };
        let credential = Credential {
            account_id: account.id,
            resource: provider::primary_resource(provider, service),
            access_token: String::new(),
            refresh_token: None,
            expires_at: None,
            scope: Vec::new(),
            token_type: "password".into(),
            secret: Some(password.expose().as_bytes().to_vec()),
            generation: account.generation,
        };
        let mut data = self.data.lock().await;
        let mut next = data.clone();
        next.accounts.push(account.clone());
        next.credentials.push(credential);
        self.store.save(&next)?;
        *data = next;
        drop(data);
        self.emit(service);
        Ok(account)
    }
}

#[cfg(feature = "calendar")]
impl Accounts {
    pub async fn add_caldav_account(
        &self,
        settings: CalDavSettings,
        username: String,
        password: SecretString,
    ) -> Result<Account> {
        validate_secure_url(&settings.url, &["https"])?;
        self.insert_password_account(
            Provider::CustomCalDav,
            Service::Calendar,
            username,
            None,
            Endpoint::CalDav(settings),
            password,
        )
        .await
    }
}

#[cfg(feature = "mail")]
impl Accounts {
    pub async fn add_imap_smtp_account(
        &self,
        settings: ImapSmtpSettings,
        username: String,
        password: SecretString,
    ) -> Result<Account> {
        if settings.imap_host.trim().is_empty()
            || settings.smtp_host.trim().is_empty()
            || settings.imap_port == 0
            || settings.smtp_port == 0
        {
            return Err(Error::InvalidEndpoint(
                "IMAP and SMTP hosts and ports are required".into(),
            ));
        }
        self.insert_password_account(
            Provider::CustomImapSmtp,
            Service::Mail,
            username,
            None,
            Endpoint::ImapSmtp(settings),
            password,
        )
        .await
    }
}

#[cfg(feature = "contacts")]
impl Accounts {
    pub async fn add_carddav_account(
        &self,
        settings: CalDavSettings,
        username: String,
        password: SecretString,
    ) -> Result<Account> {
        validate_secure_url(&settings.url, &["https"])?;
        self.insert_password_account(
            Provider::CustomCardDav,
            Service::Contacts,
            username,
            None,
            Endpoint::CardDav(settings),
            password,
        )
        .await
    }

    pub async fn add_ldap_account(
        &self,
        settings: LdapSettings,
        username: String,
        password: SecretString,
    ) -> Result<Account> {
        validate_secure_url(&settings.url, &["ldaps"])?;
        if settings.base_dn.trim().is_empty() {
            return Err(Error::InvalidEndpoint("LDAP base DN is required".into()));
        }
        self.insert_password_account(
            Provider::CustomLdap,
            Service::Contacts,
            username,
            None,
            Endpoint::Ldap(settings),
            password,
        )
        .await
    }
}

#[cfg(any(feature = "calendar", feature = "contacts"))]
fn validate_secure_url(value: &str, schemes: &[&str]) -> Result<()> {
    let url = url::Url::parse(value)
        .map_err(|_| Error::InvalidEndpoint("endpoint is not a valid URL".into()))?;
    if !schemes.contains(&url.scheme())
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::InvalidEndpoint(format!(
            "endpoint must use one of {}",
            schemes.join(", ")
        )));
    }
    Ok(())
}
