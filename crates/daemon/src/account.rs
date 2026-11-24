use crate::{
    Error, auth::AuthManager, credentials::CredentialsInterface, grants::GrantStore,
    manager::create_request, polkit, services::ServiceFactory,
};
use accounts_core::{config::AccountsConfig, models::Service};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use zbus::{
    fdo::Result, interface, message::Header, object_server::SignalEmitter,
    zvariant::OwnedObjectPath,
};

/// Per-account D-Bus object served at `/dev/edfloreshz/Accounts/Accounts/<dbus_id>`.
pub struct AccountInterface {
    pub(crate) id: Uuid,
    pub(crate) config: Arc<Mutex<AccountsConfig>>,
    pub(crate) auth_manager: Arc<Mutex<AuthManager>>,
    pub(crate) grants: GrantStore,
}

impl AccountInterface {
    pub fn new(
        id: Uuid,
        config: Arc<Mutex<AccountsConfig>>,
        auth_manager: Arc<Mutex<AuthManager>>,
        grants: GrantStore,
    ) -> Self {
        Self {
            id,
            config,
            auth_manager,
            grants,
        }
    }

    async fn current(&self) -> Result<accounts_core::models::Account> {
        let config = self.config.lock().await;
        config
            .get_account(&self.id)
            .ok_or_else(|| Error::AccountNotFound(self.id.to_string()).into())
    }
}

#[interface(name = "dev.edfloreshz.Accounts.Account")]
impl AccountInterface {
    #[zbus(property)]
    async fn id(&self) -> Result<String> {
        Ok(self.id.to_string())
    }

    #[zbus(property)]
    async fn provider_id(&self) -> Result<String> {
        Ok(self.current().await?.provider)
    }

    #[zbus(property)]
    async fn display_name(&self) -> Result<String> {
        Ok(self.current().await?.display_name)
    }

    #[zbus(property)]
    async fn set_display_name(
        &self,
        #[zbus(header)] header: Option<Header<'_>>,
        value: String,
    ) -> Result<()> {
        self.authorize_manage(header.as_ref()).await?;
        let mut account = self.current().await?;
        account.display_name = value;
        let mut config = self.config.lock().await;
        config
            .save_account(&account)
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to save account: {e}")))
    }

    #[zbus(property)]
    async fn identity(&self) -> Result<String> {
        let account = self.current().await?;
        Ok(account.email.unwrap_or(account.username))
    }

    #[zbus(property)]
    async fn enabled(&self) -> Result<bool> {
        Ok(self.current().await?.enabled)
    }

    #[zbus(property)]
    async fn set_enabled(&self, value: bool) -> Result<()> {
        let mut account = self.current().await?;
        account.enabled = value;
        {
            let mut config = self.config.lock().await;
            config
                .save_account(&account)
                .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to save account: {e}")))?;
        }
        self.emit_account_toggled(value).await;
        Ok(())
    }

    #[zbus(property)]
    async fn available_services(&self) -> Result<Vec<String>> {
        Ok(self
            .current()
            .await?
            .services
            .keys()
            .map(ToString::to_string)
            .collect())
    }

    #[zbus(property)]
    async fn enabled_services(&self) -> Result<Vec<String>> {
        Ok(self
            .current()
            .await?
            .services
            .iter()
            .filter(|(_, enabled)| **enabled)
            .map(|(service, _)| service.to_string())
            .collect())
    }

    #[zbus(property)]
    async fn created_at(&self) -> Result<String> {
        Ok(self.current().await?.created_at.to_string())
    }

    #[zbus(property)]
    async fn last_used(&self) -> Result<String> {
        Ok(self
            .current()
            .await?
            .last_used
            .map(|last_used| last_used.to_string())
            .unwrap_or_default())
    }

    #[zbus(property)]
    async fn email(&self) -> Result<String> {
        Ok(self.current().await?.email.unwrap_or_default())
    }

    async fn enable_service(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        service: &str,
    ) -> Result<()> {
        self.authorize_manage(Some(&header)).await?;
        self.set_service_enabled(&emitter, service, true).await
    }

    async fn disable_service(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        service: &str,
    ) -> Result<()> {
        self.authorize_manage(Some(&header)).await?;
        self.set_service_enabled(&emitter, service, false).await
    }

    async fn remove(&self, #[zbus(header)] header: Header<'_>) -> Result<()> {
        self.authorize_manage(Some(&header)).await?;
        let id = self.id;

        let account = self.current().await?;
        for service in ServiceFactory::create_services(&account) {
            if let Err(e) = service.remove_service().await {
                tracing::warn!("failed to remove endpoint for account {id}: {e}");
            }
        }

        {
            let mut config = self.config.lock().await;
            config
                .remove_account(&id)
                .map_err(|e| zbus::fdo::Error::Failed(format!("Account {id} not removed: {e}")))?;
        }

        {
            let auth_manager = self.auth_manager.lock().await;
            auth_manager
                .delete_credentials(&id)
                .await
                .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        }

        if let Err(e) = self.grants.clear_account(&id).await {
            tracing::warn!("failed to clear grants for removed account {id}: {e}");
        }

        if let Some(connection) = crate::CONNECTION.get() {
            let path = format!(
                "/dev/edfloreshz/Accounts/Accounts/{}",
                id.to_string().replace('-', "_")
            );
            connection
                .object_server()
                .remove::<CredentialsInterface, _>(path.clone())
                .await?;
            connection
                .object_server()
                .remove::<AccountInterface, _>(path)
                .await?;

            if let Ok(iface_ref) = connection
                .object_server()
                .interface::<_, crate::manager::ManagerInterface>(
                    "/dev/edfloreshz/Accounts/Manager",
                )
                .await
            {
                let account_path = zbus::zvariant::OwnedObjectPath::try_from(format!(
                    "/dev/edfloreshz/Accounts/Accounts/{}",
                    id.to_string().replace('-', "_")
                ))
                .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
                crate::manager::ManagerInterface::account_removed(
                    iface_ref.signal_emitter(),
                    account_path,
                )
                .await?;
            }
        }

        Ok(())
    }

    async fn list_grants(&self) -> Result<Vec<(String, String, String)>> {
        self.grants
            .list(&self.id)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Revoking a grant makes the next token request prompt again.
    async fn revoke_grant(
        &self,
        #[zbus(header)] header: Header<'_>,
        service: &str,
        caller_identity: &str,
    ) -> Result<()> {
        self.authorize_manage(Some(&header)).await?;
        let Some(service) = crate::grants::normalize_service(service) else {
            return Err(Error::InvalidService(service.to_string()).into());
        };
        self.grants
            .revoke(&self.id, service, caller_identity)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn ensure_credentials(&self) -> Result<()> {
        let mut account = self.current().await?;
        let mut auth_manager = self.auth_manager.lock().await;
        auth_manager
            .ensure_credentials(&mut account)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn get_custom_mail_settings(&self) -> Result<accounts_core::models::CustomMailSettings> {
        self.current().await?.custom_mail.ok_or_else(|| {
            zbus::fdo::Error::NotSupported("This account uses a provider manifest".into())
        })
    }

    async fn set_mail_password(
        &self,
        #[zbus(header)] header: Header<'_>,
        password: &str,
    ) -> Result<()> {
        self.authorize_manage(Some(&header)).await?;
        crate::manager::validate_password(password)?;
        let auth = self.auth_manager.lock().await;
        if self.current().await?.custom_mail.is_none() {
            return Err(zbus::fdo::Error::NotSupported(
                "This account uses OAuth".into(),
            ));
        }
        auth.set_account_password(&self.id, crate::auth::PasswordKind::Mail, password)
            .await
            .map_err(|_| {
                zbus::fdo::Error::Failed("Could not save the mail password to the keyring".into())
            })
    }

    async fn get_caldav_url(&self) -> Result<String> {
        self.current().await?.caldav_url.ok_or_else(|| {
            zbus::fdo::Error::NotSupported("This is not a custom CalDAV account".into())
        })
    }

    async fn set_caldav_password(
        &self,
        #[zbus(header)] header: Header<'_>,
        password: &str,
    ) -> Result<()> {
        self.authorize_manage(Some(&header)).await?;
        crate::manager::validate_password(password)?;
        let auth = self.auth_manager.lock().await;
        self.get_caldav_url().await?;
        auth.set_account_password(&self.id, crate::auth::PasswordKind::CalDav, password)
            .await
            .map_err(|_| {
                zbus::fdo::Error::Failed("Could not save the CalDAV password to the keyring".into())
            })
    }

    async fn reauthenticate(&self) -> Result<OwnedObjectPath> {
        let account = self.current().await?;
        if account.caldav_url.is_some() {
            return Err(zbus::fdo::Error::NotSupported(
                "Update the account password instead".into(),
            ));
        }
        create_request(
            account.provider,
            Some(self.id),
            std::collections::HashMap::new(),
            self.auth_manager.clone(),
        )
        .await
    }

    #[zbus(signal)]
    async fn services_changed(
        emitter: &SignalEmitter<'_>,
        enabled_services: Vec<String>,
    ) -> zbus::Result<()>;
}

impl AccountInterface {
    async fn emit_account_toggled(&self, enabled: bool) {
        let Some(connection) = crate::CONNECTION.get() else {
            return;
        };
        let path = crate::manager::account_object_path(&self.id);
        if let Ok(iface) = connection
            .object_server()
            .interface::<_, crate::manager::ManagerInterface>("/dev/edfloreshz/Accounts/Manager")
            .await
        {
            let _ = crate::manager::ManagerInterface::account_toggled(
                iface.signal_emitter(),
                path,
                enabled,
            )
            .await;
        }
    }

    /// Deny account mutations when the caller header is missing.
    async fn authorize_manage(&self, header: Option<&Header<'_>>) -> Result<()> {
        let authorized = match header {
            Some(header) => polkit::check(header, polkit::ACTION_MANAGE_OWN_ACCOUNTS).await,
            None => false,
        };
        if authorized {
            Ok(())
        } else {
            Err(zbus::fdo::Error::AccessDenied(format!(
                "polkit denied {}",
                polkit::ACTION_MANAGE_OWN_ACCOUNTS
            )))
        }
    }

    async fn set_service_enabled(
        &self,
        emitter: &SignalEmitter<'_>,
        service: &str,
        enabled: bool,
    ) -> Result<()> {
        let mut account = self.current().await?;
        let Some(service) = Service::from_str(service.to_string()) else {
            return Err(Error::InvalidService(service.to_string()).into());
        };
        if account.custom_mail.is_some() && service != Service::Email {
            return Err(zbus::fdo::Error::NotSupported(
                "Custom mail accounts only support Email".into(),
            ));
        }
        if account.caldav_url.is_some() && service != Service::Calendar {
            return Err(zbus::fdo::Error::NotSupported(
                "Custom CalDAV accounts only support Calendar".into(),
            ));
        }
        account.services.insert(service.clone(), enabled);

        {
            let mut config = self.config.lock().await;
            config
                .save_account(&account)
                .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to save account: {e}")))?;
        }

        if let Some(svc) = ServiceFactory::create_service(&account, &service) {
            if enabled {
                svc.add_service().await?;
            } else {
                svc.remove_service().await?;
            }
        }

        let enabled_services: Vec<String> = account
            .services
            .iter()
            .filter(|(_, enabled)| **enabled)
            .map(|(service, _)| service.to_string())
            .collect();

        Self::services_changed(emitter, enabled_services)
            .await
            .map_err(Into::into)
    }
}
