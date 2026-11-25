//! OAuth 2.0 authorization-code flow with PKCE and loopback callbacks.

use std::{collections::HashMap, time::Duration};

use chrono::{Duration as ChronoDuration, Utc};
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, TokenUrl, basic::BasicClient,
    reqwest::async_http_client,
};
use serde_json::Value;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
    task::JoinHandle,
    time::timeout,
};
use url::Url;
use uuid::Uuid;

use crate::{
    config::Config,
    error::{Error, Result},
    model::{Account, CalDavSettings, Credential, Endpoint, Provider, SecretString, Service},
    provider::{self, spec},
};

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug)]
pub struct AuthRequest {
    pub provider: Provider,
    pub service: Service,
    pub state: String,
    pub url: String,
}

pub struct CompletedAuth {
    pub account: Account,
    pub credentials: Vec<Credential>,
}

type Callback = std::result::Result<(String, String), Error>;

pub(crate) struct PendingAuth {
    provider: Provider,
    service: Service,
    redirect_uri: String,
    client_id: String,
    client_secret: Option<SecretString>,
    verifier: PkceCodeVerifier,
    receiver: oneshot::Receiver<Callback>,
    cancel: oneshot::Receiver<()>,
    task: JoinHandle<()>,
}

pub(crate) struct PendingHandle {
    cancel: oneshot::Sender<()>,
}

impl Drop for PendingAuth {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Default)]
pub struct AuthManager {
    config: Config,
    pending: HashMap<String, PendingAuth>,
    handles: HashMap<String, PendingHandle>,
}

impl AuthManager {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            pending: HashMap::new(),
            handles: HashMap::new(),
        }
    }

    /// Starts a sign-in for one provider and service.
    pub async fn begin(&mut self, provider: Provider, service: Service) -> Result<AuthRequest> {
        provider::ensure_supported(provider, service)?;
        let oauth = spec(provider)
            .and_then(|spec| spec.oauth)
            .ok_or(Error::UnsupportedProvider(provider, service))?;
        let resource = provider::primary_resource(provider, service);
        let (client_config, redirect_template) = self.config.client(provider, service, resource)?;

        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|error| Error::StorageUnavailable(format!("callback listener: {error}")))?;
        let port = listener
            .local_addr()
            .map_err(|error| Error::AuthenticationFailed(error.to_string()))?
            .port();
        let mut redirect = Url::parse(&redirect_template)
            .map_err(|error| Error::InvalidClientConfig(error.to_string()))?;
        redirect
            .set_port(Some(port))
            .map_err(|_| Error::InvalidClientConfig("redirect URI cannot carry a port".into()))?;
        let redirect_uri = redirect.to_string();

        let client = BasicClient::new(
            ClientId::new(client_config.client_id.clone()),
            client_config
                .client_secret
                .as_ref()
                .map(|secret| ClientSecret::new(secret.expose().to_owned())),
            AuthUrl::new(oauth.auth_url.to_owned())
                .map_err(|error| Error::InvalidClientConfig(error.to_string()))?,
            Some(
                TokenUrl::new(oauth.token_url.to_owned())
                    .map_err(|error| Error::InvalidClientConfig(error.to_string()))?,
            ),
        )
        .set_redirect_uri(
            RedirectUrl::new(redirect_uri.clone())
                .map_err(|error| Error::InvalidClientConfig(error.to_string()))?,
        );

        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut authorization = client
            .authorize_url(CsrfToken::new_random)
            .set_pkce_challenge(challenge);
        for scope in provider::authorization_scopes(provider, service) {
            authorization = authorization.add_scope(Scope::new(scope));
        }
        for (key, value) in oauth.extra_params {
            authorization = authorization.add_extra_param((*key).to_owned(), (*value).to_owned());
        }
        let (url, csrf) = authorization.url();
        let state = csrf.secret().clone();

        let (sender, receiver) = oneshot::channel();
        let (cancel_sender, cancel_receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            let result = receive_callback(listener).await;
            let _ = sender.send(result);
        });

        self.pending.insert(
            state.clone(),
            PendingAuth {
                provider,
                service,
                redirect_uri,
                client_id: client_config.client_id,
                client_secret: client_config.client_secret,
                verifier,
                receiver,
                cancel: cancel_receiver,
                task,
            },
        );
        self.handles.insert(
            state.clone(),
            PendingHandle {
                cancel: cancel_sender,
            },
        );

        Ok(AuthRequest {
            provider,
            service,
            state,
            url: url.to_string(),
        })
    }

    /// Waits for the browser callback, validates state, exchanges the code and
    /// returns an account plus its initial credentials.
    pub(crate) fn take(&mut self, state: &str) -> Option<(PendingAuth, PendingHandle)> {
        let pending = self.pending.remove(state)?;
        let handle = self.handles.remove(state)?;
        Some((pending, handle))
    }

    pub(crate) async fn complete(
        mut pending: PendingAuth,
        _handle: PendingHandle,
        request: AuthRequest,
    ) -> Result<CompletedAuth> {
        let (code, callback_state) = tokio::select! {
            result = timeout(CALLBACK_TIMEOUT, &mut pending.receiver) => match result {
                Ok(Ok(result)) => result?,
                Ok(Err(_)) => return Err(Error::Cancelled),
                Err(_) => return Err(Error::CallbackTimeout),
            },
            _ = &mut pending.cancel => return Err(Error::Cancelled),
        };
        if callback_state != request.state {
            return Err(Error::InvalidState);
        }

        let oauth = spec(pending.provider).and_then(|spec| spec.oauth).ok_or(
            Error::UnsupportedProvider(pending.provider, pending.service),
        )?;
        let client = BasicClient::new(
            ClientId::new(pending.client_id.clone()),
            pending
                .client_secret
                .as_ref()
                .map(|secret| ClientSecret::new(secret.expose().to_owned())),
            AuthUrl::new(oauth.auth_url.to_owned())
                .map_err(|error| Error::InvalidClientConfig(error.to_string()))?,
            Some(
                TokenUrl::new(oauth.token_url.to_owned())
                    .map_err(|error| Error::InvalidClientConfig(error.to_string()))?,
            ),
        )
        .set_redirect_uri(
            RedirectUrl::new(pending.redirect_uri.clone())
                .map_err(|error| Error::InvalidClientConfig(error.to_string()))?,
        );

        let token = client
            .exchange_code(AuthorizationCode::new(code))
            .set_pkce_verifier(PkceCodeVerifier::new(pending.verifier.secret().clone()))
            .request_async(async_http_client)
            .await
            .map_err(|error| Error::AuthenticationFailed(error.to_string()))?;

        let access_token = token.access_token().secret().clone();
        let refresh_token = token
            .refresh_token()
            .map(|token| SecretString::new(token.secret().clone()))
            .ok_or_else(|| {
                Error::AuthenticationFailed("the provider did not return a refresh token".into())
            })?;
        let expires_at = token
            .expires_in()
            .map(|duration| Utc::now() + ChronoDuration::seconds(duration.as_secs() as i64));

        let identity = user_info(oauth.userinfo_url, &oauth, &access_token).await?;
        let id = Uuid::new_v4();
        let account = Account {
            id,
            provider: pending.provider,
            service: pending.service,
            display_name: identity.display_name,
            username: identity.username,
            email: identity.email,
            enabled: true,
            created_at: Utc::now(),
            generation: 1,
            endpoint: provider_endpoint(pending.provider, pending.service),
        };

        let resources = provider::resources_for(pending.provider, pending.service);
        let mut credentials = Vec::with_capacity(resources.len());
        for (index, resource) in resources.into_iter().enumerate() {
            let scope = provider::resource_scopes(pending.provider, pending.service, resource);
            credentials.push(Credential {
                account_id: id,
                resource,
                access_token: if index == 0 {
                    access_token.clone()
                } else {
                    String::new()
                },
                refresh_token: Some(refresh_token.clone()),
                expires_at: if index == 0 { expires_at } else { None },
                scope,
                token_type: "Bearer".into(),
                secret: None,
                generation: account.generation,
            });
        }

        Ok(CompletedAuth {
            account,
            credentials,
        })
    }

    pub fn cancel(&mut self, state: &str) {
        if let Some(handle) = self.handles.remove(state) {
            let _ = handle.cancel.send(());
        }
        if let Some(pending) = self.pending.remove(state) {
            drop(pending);
        }
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

struct Identity {
    display_name: String,
    username: String,
    email: Option<String>,
}

async fn user_info(url: &str, oauth: &provider::OAuthSpec, token: &str) -> Result<Identity> {
    let response = reqwest::Client::new()
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| Error::Network(error.to_string()))?
        .error_for_status()
        .map_err(|error| Error::ProviderResponse(error.to_string()))?
        .json::<Value>()
        .await
        .map_err(|error| Error::ProviderResponse(error.to_string()))?;

    let field = |name: &str| {
        response
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .filter(|value| !value.is_empty())
    };
    let email = field(oauth.email_field);
    let username = oauth
        .username_field
        .and_then(field)
        .or_else(|| email.clone())
        .unwrap_or_else(|| "Unknown".into());
    let display_name = field(oauth.display_name_field)
        .or_else(|| email.clone())
        .unwrap_or_else(|| username.clone());

    Ok(Identity {
        display_name,
        username,
        email,
    })
}

fn provider_endpoint(provider: Provider, service: Service) -> Endpoint {
    match provider::endpoint_base(provider, service) {
        Some(url) if matches!(service, Service::Calendar | Service::Contacts) => {
            Endpoint::CalDav(CalDavSettings {
                url: url.to_owned(),
            })
        }
        _ => Endpoint::None,
    }
}

async fn receive_callback(listener: TcpListener) -> Callback {
    let (mut stream, _) = timeout(CALLBACK_TIMEOUT, listener.accept())
        .await
        .map_err(|_| Error::CallbackTimeout)?
        .map_err(|error| Error::AuthenticationFailed(error.to_string()))?;

    let mut buffer = [0u8; 8192];
    let read = timeout(CALLBACK_TIMEOUT, stream.read(&mut buffer))
        .await
        .map_err(|_| Error::CallbackTimeout)?
        .map_err(|error| Error::AuthenticationFailed(error.to_string()))?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let query = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|target| target.split_once('?'))
        .map(|(_, query)| query.to_owned())
        .unwrap_or_default();

    let body = "<!DOCTYPE html><html><body><p>You can close this window and return to the app.</p></body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;

    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut error_description = None;
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            "error_description" => error_description = Some(value.into_owned()),
            _ => {}
        }
    }

    if let Some(error) = error {
        return Err(Error::AuthenticationFailed(
            error_description.unwrap_or(error),
        ));
    }
    let code = code.ok_or_else(|| Error::AuthenticationFailed("callback had no code".into()))?;
    let state = state.ok_or(Error::InvalidState)?;
    Ok((code, state))
}
