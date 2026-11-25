//! Service-specific credential types returned to consuming applications.

use crate::{
    error::{Error, Result},
    manager::Accounts,
    model::{Provider, Resource, SecretString, Service},
    provider,
};

fn password_from(secret: Option<Vec<u8>>) -> Result<SecretString> {
    let bytes = secret.ok_or_else(|| Error::AuthenticationFailed("password is missing".into()))?;
    let password = String::from_utf8(bytes)
        .map_err(|_| Error::AuthenticationFailed("password is not valid UTF-8".into()))?;
    Ok(SecretString::new(password))
}

#[cfg(feature = "calendar")]
pub enum CalendarCredentials {
    OAuth {
        uri: String,
        token: SecretString,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    },
    Basic {
        url: String,
        username: String,
        password: SecretString,
    },
}

#[cfg(feature = "calendar")]
impl Accounts {
    pub async fn calendar_credentials(&self, id: crate::model::AccountId) -> Result<CalendarCredentials> {
        let account = self
            .get(id)
            .await
            .ok_or_else(|| Error::AccountNotFound(id.to_string()))?;
        if !account.enabled || account.service != Service::Calendar {
            return Err(Error::NeedsReauth(id.to_string()));
        }

        if account.provider == Provider::CustomCalDav {
            let credential = self
                .credential(id, Resource::CalDav)
                .await?;
            let url = match &account.endpoint {
                crate::model::Endpoint::CalDav(settings) => settings.url.clone(),
                _ => return Err(Error::InvalidEndpoint("CalDAV URL is missing".into())),
            };
            return Ok(CalendarCredentials::Basic {
                url,
                username: account.username,
                password: password_from(credential.secret.clone())?,
            });
        }

        let credential = self
            .credential(id, provider::primary_resource(account.provider, Service::Calendar))
            .await?;
        let uri = match &account.endpoint {
            crate::model::Endpoint::CalDav(settings) => settings.url.clone(),
            _ => provider::endpoint_base(account.provider, Service::Calendar)
                .unwrap_or_default()
                .to_owned(),
        };
        Ok(CalendarCredentials::OAuth {
            uri,
            token: SecretString::new(credential.access_token.clone()),
            expires_at: credential.expires_at,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailTransport {
    Graph,
    ImapSmtp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailConnectionSettings {
    pub transport: MailTransport,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_tls: bool,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_starttls: bool,
}

#[cfg(feature = "mail")]
pub enum MailCredentials {
    OAuth {
        settings: MailConnectionSettings,
        token: SecretString,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    },
    Password {
        settings: MailConnectionSettings,
        username: String,
        password: SecretString,
    },
}

#[cfg(feature = "mail")]
fn provider_mail_settings(provider: Provider) -> Option<MailConnectionSettings> {
    match provider {
        Provider::Google => Some(MailConnectionSettings {
            transport: MailTransport::ImapSmtp,
            imap_host: "imap.gmail.com".into(),
            imap_port: 993,
            imap_tls: true,
            smtp_host: "smtp.gmail.com".into(),
            smtp_port: 587,
            smtp_starttls: true,
        }),
        Provider::Microsoft => Some(MailConnectionSettings {
            transport: MailTransport::ImapSmtp,
            imap_host: "outlook.office365.com".into(),
            imap_port: 993,
            imap_tls: true,
            smtp_host: "smtp.office365.com".into(),
            smtp_port: 587,
            smtp_starttls: true,
        }),
        _ => None,
    }
}

#[cfg(feature = "mail")]
impl Accounts {
    /// Graph bearer token for Microsoft's Graph transport.
    pub async fn mail_graph_token(
        &self,
        id: crate::model::AccountId,
    ) -> Result<SecretString> {
        let credential = self.credential(id, Resource::Graph).await?;
        Ok(SecretString::new(credential.access_token.clone()))
    }

    /// IMAP/SMTP credentials. Microsoft uses a separate Exchange token; a
    /// Graph token is never returned here.
    pub async fn mail_access(
        &self,
        id: crate::model::AccountId,
    ) -> Result<MailCredentials> {
        let account = self
            .get(id)
            .await
            .ok_or_else(|| Error::AccountNotFound(id.to_string()))?;
        if !account.enabled || account.service != Service::Mail {
            return Err(Error::NeedsReauth(id.to_string()));
        }

        let settings = match &account.endpoint {
            crate::model::Endpoint::ImapSmtp(settings) => MailConnectionSettings {
                transport: MailTransport::ImapSmtp,
                imap_host: settings.imap_host.clone(),
                imap_port: settings.imap_port,
                imap_tls: settings.imap_tls,
                smtp_host: settings.smtp_host.clone(),
                smtp_port: settings.smtp_port,
                smtp_starttls: settings.smtp_starttls,
            },
            _ => provider_mail_settings(account.provider)
                .ok_or_else(|| Error::UnsupportedProvider(account.provider, Service::Mail))?,
        };

        let resource = if account.provider == Provider::Microsoft {
            Resource::Exchange
        } else {
            Resource::Primary
        };
        let credential = self.credential(id, resource).await?;
        if credential.token_type == "password" {
            return Ok(MailCredentials::Password {
                settings,
                username: account.username,
                password: password_from(credential.secret.clone())?,
            });
        }
        Ok(MailCredentials::OAuth {
            settings,
            token: SecretString::new(credential.access_token.clone()),
            expires_at: credential.expires_at,
        })
    }
}

#[cfg(feature = "contacts")]
pub enum ContactsCredentials {
    OAuth {
        endpoint: String,
        token: SecretString,
    },
    Basic {
        settings: crate::model::ContactsConnectionSettings,
        username: String,
        password: SecretString,
    },
}

#[cfg(feature = "contacts")]
impl Accounts {
    pub async fn contacts_credentials(
        &self,
        id: crate::model::AccountId,
    ) -> Result<ContactsCredentials> {
        let account = self
            .get(id)
            .await
            .ok_or_else(|| Error::AccountNotFound(id.to_string()))?;
        if !account.enabled || account.service != Service::Contacts {
            return Err(Error::NeedsReauth(id.to_string()));
        }
        let resource = provider::primary_resource(account.provider, Service::Contacts);
        let credential = self.credential(id, resource).await?;
        if credential.token_type == "password" {
            let settings = match &account.endpoint {
                crate::model::Endpoint::CardDav(settings) => {
                    crate::model::ContactsConnectionSettings {
                        protocol: "carddav".into(),
                        uri: settings.url.clone(),
                        base_dn: String::new(),
                    }
                }
                crate::model::Endpoint::Ldap(settings) => {
                    crate::model::ContactsConnectionSettings {
                        protocol: "ldap".into(),
                        uri: settings.url.clone(),
                        base_dn: settings.base_dn.clone(),
                    }
                }
                _ => return Err(Error::InvalidEndpoint("contacts endpoint is missing".into())),
            };
            return Ok(ContactsCredentials::Basic {
                settings,
                username: account.username,
                password: password_from(credential.secret.clone())?,
            });
        }
        Ok(ContactsCredentials::OAuth {
            endpoint: provider::endpoint_base(account.provider, Service::Contacts)
                .unwrap_or_default()
                .to_owned(),
            token: SecretString::new(credential.access_token.clone()),
        })
    }
}

#[cfg(feature = "tasks")]
pub struct TasksCredentials {
    pub endpoint: Option<String>,
    pub token: SecretString,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(feature = "tasks")]
impl Accounts {
    pub async fn tasks_credentials(
        &self,
        id: crate::model::AccountId,
    ) -> Result<TasksCredentials> {
        let account = self
            .get(id)
            .await
            .ok_or_else(|| Error::AccountNotFound(id.to_string()))?;
        if !account.enabled || account.service != Service::Tasks {
            return Err(Error::NeedsReauth(id.to_string()));
        }
        let credential = self
            .credential(id, provider::primary_resource(account.provider, Service::Tasks))
            .await?;
        Ok(TasksCredentials {
            endpoint: provider::endpoint_base(account.provider, Service::Tasks).map(str::to_owned),
            token: SecretString::new(credential.access_token.clone()),
            expires_at: credential.expires_at,
        })
    }
}
