//! Issues credentials after polkit authorization and per-app, per-service consent.

use std::collections::HashSet;
use std::sync::Arc;

use accounts_core::{
    config::AccountsConfig,
    models::{Account, Service},
};
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

    async fn enabled_account(&self, service: &Service) -> std::result::Result<Account, TokenError> {
        let account = self
            .config
            .lock()
            .await
            .get_account(&self.id)
            .ok_or_else(|| {
                TokenError::Failed(Error::AccountNotFound(self.id.to_string()).to_string())
            })?;

        if !account.enabled || account.services.get(service) != Some(&true) {
            return Err(TokenError::Disabled(format!(
                "{} is disabled for account {}",
                service, self.id
            )));
        }
        Ok(account)
    }
}

#[interface(name = "dev.edfloreshz.Accounts.Credentials")]
impl CredentialsInterface {
    #[zbus(property)]
    async fn auth_method(&self) -> Result<String> {
        let account = self.config.lock().await.get_account(&self.id);
        let method = match account {
            Some(account) if account.caldav_url.is_some() => "basic",
            Some(account) if account.custom_mail.is_some() => "password",
            Some(account) if account.custom_contacts.is_some() => {
                crate::services::contacts_auth_method(&account)
            }
            _ => "oauth2",
        };
        Ok(method.into())
    }

    async fn get_access_token(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        service: &str,
    ) -> std::result::Result<(String, i64), TokenError> {
        self.issue_access_token(header, connection, service, false)
            .await
    }

    async fn get_mail_access(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> std::result::Result<(accounts_core::proxy::MailConnectionSettings, String, i64), TokenError>
    {
        let account = self.enabled_account(&Service::Email).await?;
        let settings = crate::services::mail_settings(&account)
            .map_err(|err| TokenError::Disabled(err.to_string()))?;
        if account.custom_mail.is_some() {
            return Err(TokenError::Disabled(
                "This account uses a mail password".into(),
            ));
        }
        let (token, expiry) = self
            .issue_access_token(header, connection, "Email", true)
            .await?;
        Ok((settings, token, expiry))
    }

    async fn get_mail_password(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> std::result::Result<
        (accounts_core::proxy::MailConnectionSettings, String, String),
        TokenError,
    > {
        let account = self.enabled_account(&Service::Email).await?;
        if account.custom_mail.is_none() {
            return Err(TokenError::Disabled("This account uses OAuth".into()));
        }
        self.authorize(header, connection, "Email").await?;
        let auth = self.auth_manager.lock().await;
        let account = self.enabled_account(&Service::Email).await?;
        let settings = crate::services::mail_settings(&account)
            .map_err(|err| TokenError::Disabled(err.to_string()))?;
        let password = auth
            .account_password(&self.id, crate::auth::PasswordKind::Mail)
            .await
            .map_err(|_| {
                TokenError::NeedsReauth("Update the password in COSMIC Accounts".into())
            })?;
        Ok((settings, account.username, password))
    }

    async fn get_caldav_access(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> std::result::Result<(String, String, String), TokenError> {
        let account = self.enabled_account(&Service::Calendar).await?;
        if account.caldav_url.is_none() {
            return Err(TokenError::Disabled(
                "This is not a custom CalDAV account".into(),
            ));
        }
        self.authorize(header, connection, "Calendar").await?;
        let auth = self.auth_manager.lock().await;
        let account = self.enabled_account(&Service::Calendar).await?;
        let url = account
            .caldav_url
            .ok_or_else(|| TokenError::Disabled("This is not a custom CalDAV account".into()))?;
        let password = auth
            .account_password(&self.id, crate::auth::PasswordKind::CalDav)
            .await
            .map_err(|_| {
                TokenError::NeedsReauth("Update the password in COSMIC Accounts".into())
            })?;
        Ok((url, account.username, password))
    }

    async fn get_contact_access(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> std::result::Result<
        (
            accounts_core::models::ContactsConnectionSettings,
            String,
            String,
        ),
        TokenError,
    > {
        let account = self.enabled_account(&Service::Contacts).await?;
        if account.custom_contacts.is_none() {
            return Err(TokenError::Disabled(
                "This is not a custom contacts account".into(),
            ));
        }
        self.authorize(header, connection, "Contacts").await?;
        let auth = self.auth_manager.lock().await;
        let account = self.enabled_account(&Service::Contacts).await?;
        let settings = account
            .custom_contacts
            .ok_or_else(|| TokenError::Disabled("This is not a custom contacts account".into()))?;
        let password = auth
            .account_password(&self.id, crate::auth::PasswordKind::Contacts)
            .await
            .map_err(|_| {
                TokenError::NeedsReauth("Update the password in COSMIC Accounts".into())
            })?;
        Ok((settings, account.username, password))
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
    async fn authorize(
        &self,
        header: Header<'_>,
        connection: &zbus::Connection,
        service: &str,
    ) -> std::result::Result<Account, TokenError> {
        if !polkit::check(&header, polkit::ACTION_GET_TOKEN).await {
            return Err(TokenError::AccessDenied(format!(
                "polkit denied {}",
                polkit::ACTION_GET_TOKEN
            )));
        }

        let Some(service) = crate::grants::normalize_service(service) else {
            return Err(TokenError::Failed(format!("unknown service {service:?}")));
        };

        let account_service = Service::from_str(service.to_owned())
            .ok_or_else(|| TokenError::Failed(format!("unknown service {service:?}")))?;
        let account = self.enabled_account(&account_service).await?;

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

        self.enabled_account(&account_service).await
    }

    async fn issue_access_token(
        &self,
        header: Header<'_>,
        connection: &zbus::Connection,
        service: &str,
        oauth_only: bool,
    ) -> std::result::Result<(String, i64), TokenError> {
        let mut account = self.authorize(header, connection, service).await?;
        if account.custom_mail.is_some()
            || account.caldav_url.is_some()
            || account.custom_contacts.is_some()
        {
            return Err(TokenError::Disabled(
                "This account uses password authentication".into(),
            ));
        }
        let force_refresh = self.stale.lock().await.remove(&self.id);
        let mut auth_manager = self.auth_manager.lock().await;

        if let Ok(credential) = auth_manager.get_account_credentials(&self.id).await
            && credential.token_type == "handler"
        {
            if oauth_only {
                return Err(TokenError::NeedsReauth(
                    "Reconnect the account to obtain an OAuth token".into(),
                ));
            }
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
