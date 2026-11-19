//! Issues credentials after polkit authorization and per-app, per-service consent.

use std::collections::HashSet;
use std::sync::Arc;

use accounts_core::config::AccountsConfig;
use tokio::sync::Mutex;
use uuid::Uuid;
use zbus::{fdo::Result, interface, message::Header};

use crate::{
    Error,
    auth::AuthManager,
    caller,
    consent::{self, ConsentError},
    error::TokenError,
    grants::{Decision, GrantStore},
    polkit,
};

pub struct CredentialsInterface {
    pub(crate) id: Uuid,
    pub(crate) config: Arc<Mutex<AccountsConfig>>,
    pub(crate) auth_manager: Arc<Mutex<AuthManager>>,
    pub(crate) grants: GrantStore,
    /// Accounts whose next token request must force a refresh.
    pub(crate) stale: Arc<Mutex<HashSet<Uuid>>>,
}

impl CredentialsInterface {
    pub fn new(
        id: Uuid,
        config: Arc<Mutex<AccountsConfig>>,
        auth_manager: Arc<Mutex<AuthManager>>,
        grants: GrantStore,
        stale: Arc<Mutex<HashSet<Uuid>>>,
    ) -> Self {
        Self {
            id,
            config,
            auth_manager,
            grants,
            stale,
        }
    }

    async fn current(&self) -> std::result::Result<accounts_core::models::Account, Error> {
        let config = self.config.lock().await;
        config
            .get_account(&self.id)
            .ok_or_else(|| Error::AccountNotFound(self.id.to_string()))
    }
}

#[interface(name = "dev.edfloreshz.Accounts.Credentials")]
impl CredentialsInterface {
    #[zbus(property)]
    async fn auth_method(&self) -> Result<String> {
        Ok("oauth2".to_string())
    }

    async fn get_access_token(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        service: &str,
    ) -> std::result::Result<(String, i64), TokenError> {
        if !polkit::check(&header, polkit::ACTION_GET_TOKEN).await {
            return Err(TokenError::AccessDenied(format!(
                "polkit denied {}",
                polkit::ACTION_GET_TOKEN
            )));
        }

        let Some(service) = crate::grants::normalize_service(service) else {
            return Err(TokenError::Failed(format!("unknown service {service:?}")));
        };

        let mut account = self
            .current()
            .await
            .map_err(|e| TokenError::Failed(e.to_string()))?;

        if !account.enabled {
            return Err(TokenError::Disabled(format!(
                "Account {} is disabled",
                self.id
            )));
        }

        let who = caller::resolve(connection, &header).await;
        match who.identity.as_deref() {
            Some(identity) => {
                match self
                    .grants
                    .lookup(&self.id, service, identity)
                    .await
                    .map_err(|e| TokenError::Failed(e.to_string()))?
                {
                    Some(Decision::Allow) => {}
                    Some(Decision::Deny) => {
                        return Err(TokenError::AccessDenied(
                            "caller was previously denied access to this account".to_string(),
                        ));
                    }
                    None => {
                        let decision = self
                            .ask(connection, &account, &who.display_name, service)
                            .await?;
                        self.grants
                            .put(
                                &self.id,
                                service,
                                identity,
                                if decision {
                                    Decision::Allow
                                } else {
                                    Decision::Deny
                                },
                            )
                            .await
                            .map_err(|e| TokenError::Failed(e.to_string()))?;
                        if !decision {
                            return Err(TokenError::AccessDenied(
                                "the user denied this request".to_string(),
                            ));
                        }
                    }
                }
            }
            None => {
                // Unidentifiable caller: prompt every time, never persist a grant.
                if !self
                    .ask(connection, &account, &who.display_name, service)
                    .await?
                {
                    return Err(TokenError::AccessDenied(
                        "the user denied this request".to_string(),
                    ));
                }
            }
        }

        let force_refresh = self.stale.lock().await.remove(&self.id);
        let mut auth_manager = self.auth_manager.lock().await;

        if let Ok(credential) = auth_manager.get_account_credentials(&self.id).await
            && credential.token_type == "handler"
        {
            let credential = if force_refresh {
                auth_manager
                    .refresh_handler_credentials(&self.id)
                    .await
                    .map_err(|e| {
                        TokenError::NeedsReauth(format!(
                            "provider handler could not refresh, call Reauthenticate: {e}"
                        ))
                    })?
            } else {
                credential
            };
            let blob = credential.credential_blob.unwrap_or_default();
            let expires_at = credential.expires_at.map(|at| at.timestamp()).unwrap_or(0);
            return Ok((String::from_utf8_lossy(&blob).into_owned(), expires_at));
        }

        let refresh = if force_refresh {
            auth_manager.refresh_token(&account).await
        } else {
            auth_manager.ensure_credentials(&mut account).await
        };
        if let Err(e) = refresh {
            return Err(TokenError::NeedsReauth(format!(
                "credential refresh failed, call Reauthenticate: {e}"
            )));
        }

        let credential = auth_manager
            .get_account_credentials(&account.id)
            .await
            .map_err(|e| TokenError::NeedsReauth(e.to_string()))?;
        let expires_at = credential.expires_at.map(|at| at.timestamp()).unwrap_or(0);
        Ok((credential.access_token, expires_at))
    }

    /// Force a refresh on the next token request after a provider returns 401.
    async fn invalidate_token(&self, #[zbus(header)] header: Header<'_>) -> Result<()> {
        if !polkit::check(&header, polkit::ACTION_GET_TOKEN).await {
            return Err(zbus::fdo::Error::AccessDenied(format!(
                "polkit denied {}",
                polkit::ACTION_GET_TOKEN
            )));
        }
        self.stale.lock().await.insert(self.id);
        Ok(())
    }
}

impl CredentialsInterface {
    async fn ask(
        &self,
        connection: &zbus::Connection,
        account: &accounts_core::models::Account,
        caller_name: &str,
        service: &str,
    ) -> std::result::Result<bool, TokenError> {
        match consent::prompt(
            connection,
            caller_name,
            &account.display_name,
            &account.provider,
            service,
        )
        .await
        {
            Ok(decision) => Ok(decision),
            Err(ConsentError::Timeout) => Err(TokenError::ConsentTimeout(
                "no response to the consent prompt".to_string(),
            )),
            Err(ConsentError::Failed(reason)) => Err(TokenError::Failed(format!(
                "consent prompt failed: {reason}"
            ))),
        }
    }
}
