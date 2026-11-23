use accounts_core::{
    AccountService, ProviderManifest,
    models::{Account, Service},
    proxy::MailConnectionSettings,
    registry::{MailEndpointManifest, MailTls},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use zbus::{
    fdo::{Error, Result},
    interface,
};

use crate::CONNECTION;
use crate::services::{endpoint_object_path, provider_manifest, refresh_account_credentials};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MailService {
    account: Account,
}

impl MailService {
    pub fn new(account: Account) -> Self {
        Self { account }
    }
}

fn trusted_endpoint(manifest: &ProviderManifest) -> Option<&MailEndpointManifest> {
    let endpoint = manifest.endpoint.mail.as_ref()?;
    // Provider overrides may supply OAuth configuration. They cannot redirect
    // Gmail bearer tokens to another server or weaken transport security.
    let trusted = manifest.provider.id == "google"
        && manifest.handler.is_none()
        && manifest
            .oauth
            .scopes
            .iter()
            .any(|scope| scope == "https://mail.google.com/")
        && endpoint.imap_host == "imap.gmail.com"
        && endpoint.imap_port == 993
        && endpoint.imap_tls == MailTls::Tls
        && endpoint.smtp_host == "smtp.gmail.com"
        && matches!(
            (endpoint.smtp_port, endpoint.smtp_tls()),
            (465, MailTls::Tls) | (587, MailTls::Starttls)
        );
    trusted.then_some(endpoint)
}

pub(crate) fn mail_endpoint(account: &Account) -> Result<&'static MailEndpointManifest> {
    trusted_endpoint(provider_manifest(account)?).ok_or_else(|| {
        Error::NotSupported(format!(
            "Provider {} has no trusted OAuth IMAP/SMTP endpoint",
            account.provider
        ))
    })
}

pub(crate) fn has_mail_endpoint(account: &Account) -> bool {
    account.custom_mail.is_some() || mail_endpoint(account).is_ok()
}

pub(crate) fn mail_settings(account: &Account) -> Result<MailConnectionSettings> {
    if let Some(settings) = &account.custom_mail {
        return Ok(MailConnectionSettings {
            imap_host: settings.imap_host.clone(),
            imap_port: settings.imap_port,
            imap_tls: "tls".into(),
            smtp_host: settings.smtp_host.clone(),
            smtp_port: settings.smtp_port,
            smtp_tls: if settings.smtp_starttls {
                "starttls"
            } else {
                "tls"
            }
            .into(),
            authentication: "password".into(),
        });
    }
    let endpoint = mail_endpoint(account)?;
    Ok(MailConnectionSettings {
        imap_host: endpoint.imap_host.clone(),
        imap_port: endpoint.imap_port,
        imap_tls: endpoint.imap_tls.as_str().into(),
        smtp_host: endpoint.smtp_host.clone(),
        smtp_port: endpoint.smtp_port,
        smtp_tls: endpoint.smtp_tls().as_str().into(),
        authentication: "xoauth2".into(),
    })
}

#[interface(name = "dev.edfloreshz.Accounts.Endpoint.Mail")]
impl MailService {
    async fn get_connection_settings(&self) -> Result<MailConnectionSettings> {
        mail_settings(&self.account)
    }

    #[zbus(property)]
    async fn imap_host(&self) -> Result<String> {
        Ok(mail_settings(&self.account)?.imap_host)
    }

    #[zbus(property)]
    async fn imap_port(&self) -> Result<u16> {
        Ok(mail_settings(&self.account)?.imap_port)
    }

    #[zbus(property)]
    async fn smtp_host(&self) -> Result<String> {
        Ok(mail_settings(&self.account)?.smtp_host)
    }

    #[zbus(property)]
    async fn smtp_port(&self) -> Result<u16> {
        Ok(mail_settings(&self.account)?.smtp_port)
    }

    #[zbus(property)]
    async fn auth_method(&self) -> Result<String> {
        Ok(if self.account.custom_mail.is_some() {
            "password"
        } else {
            "oauth2"
        }
        .into())
    }
}

#[async_trait]
impl AccountService for MailService {
    fn name(&self) -> &str {
        "Mail"
    }

    fn interface_name(&self) -> &str {
        "dev.edfloreshz.Accounts.Endpoint.Mail"
    }

    fn is_supported(&self, account: &Account) -> bool {
        account.services.contains_key(&Service::Email) && has_mail_endpoint(account)
    }

    async fn add_service(&self) -> Result<bool> {
        tracing::info!(
            "Adding the mail endpoint for account {}",
            self.account.dbus_id()
        );
        if let Some(connection) = CONNECTION.get() {
            connection
                .object_server()
                .at(endpoint_object_path(&self.account), self.clone())
                .await?;
        }
        Ok(false)
    }

    async fn remove_service(&self) -> Result<bool> {
        tracing::info!(
            "Removing the mail endpoint for account {}",
            self.account.dbus_id()
        );
        if let Some(connection) = CONNECTION.get() {
            connection
                .object_server()
                .remove::<MailService, String>(endpoint_object_path(&self.account))
                .await?;
        }
        Ok(false)
    }

    async fn ensure_credentials(&self, account: &mut Account) -> Result<()> {
        refresh_account_credentials(account).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publishes_mail_connection_settings_over_dbus() {
        use accounts_core::{ProviderRegistry, Uuid, proxy::MailProxy};
        use std::path::PathBuf;
        use zbus::Connection;

        let providers = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ui/data/providers");
        crate::REGISTRY
            .set(ProviderRegistry::load(&[providers]))
            .unwrap();
        let account = Account {
            id: Uuid::new_v4(),
            provider: "google".into(),
            display_name: "Test account".into(),
            username: "test@example.org".into(),
            email: Some("test@example.org".into()),
            enabled: true,
            created_at: chrono::Utc::now(),
            last_used: None,
            services: [(Service::Email, true)].into(),
            custom_mail: None,
        };
        let path = endpoint_object_path(&account);
        let server = zbus::connection::Builder::session()
            .unwrap()
            .serve_at(path.as_str(), MailService::new(account))
            .unwrap()
            .build()
            .await
            .unwrap();
        let connection = Connection::session().await.unwrap();
        let proxy = MailProxy::builder(&connection)
            .destination(server.unique_name().unwrap().clone())
            .unwrap()
            .path(path)
            .unwrap()
            .build()
            .await
            .unwrap();

        let settings = proxy.get_connection_settings().await.unwrap();
        assert_eq!(
            (
                settings.imap_host.as_str(),
                settings.imap_port,
                settings.imap_tls.as_str()
            ),
            ("imap.gmail.com", 993, "tls")
        );
        assert_eq!(
            (
                settings.smtp_host.as_str(),
                settings.smtp_port,
                settings.smtp_tls.as_str()
            ),
            ("smtp.gmail.com", 587, "starttls")
        );
        assert_eq!(settings.authentication, "xoauth2");
    }

    #[test]
    fn accepts_legacy_gmail_settings_and_rejects_redirected_tokens() {
        let mut manifest: ProviderManifest =
            toml::from_str(include_str!("../../../ui/data/providers/google.toml")).unwrap();
        let mail = manifest.endpoint.mail.as_mut().unwrap();
        mail.smtp_tls = None;
        assert!(trusted_endpoint(&manifest).is_some());

        let mail = manifest.endpoint.mail.as_mut().unwrap();
        mail.smtp_tls = Some(MailTls::Tls);
        assert!(trusted_endpoint(&manifest).is_none());

        let mail = manifest.endpoint.mail.as_mut().unwrap();
        mail.smtp_port = 465;
        assert!(trusted_endpoint(&manifest).is_some());

        let mail = manifest.endpoint.mail.as_mut().unwrap();
        mail.smtp_port = 587;
        mail.smtp_tls = Some(MailTls::Starttls);
        mail.smtp_host = "mail.example.org".into();
        assert!(trusted_endpoint(&manifest).is_none());
    }

    #[test]
    fn does_not_offer_imap_or_smtp_for_graph_tokens() {
        let mut manifest: ProviderManifest =
            toml::from_str(include_str!("../../../ui/data/providers/microsoft.toml")).unwrap();
        let google: ProviderManifest =
            toml::from_str(include_str!("../../../ui/data/providers/google.toml")).unwrap();
        manifest.endpoint.mail = google.endpoint.mail;
        assert!(trusted_endpoint(&manifest).is_none());
    }
}
