mod calendar;
mod contacts;
mod mail;
mod tasks;

use accounts_core::{
    AccountService,
    models::{Account, Service},
};
pub use calendar::*;
pub use contacts::*;
pub use mail::*;
pub use tasks::*;
use zbus::fdo::{Error, Result};

/// Endpoint interfaces share the account's D-Bus path.
pub(crate) fn endpoint_object_path(account: &Account) -> String {
    format!("/dev/edfloreshz/Accounts/Accounts/{}", account.dbus_id())
}

pub(crate) fn provider_manifest(
    account: &Account,
) -> Result<&'static accounts_core::ProviderManifest> {
    crate::REGISTRY
        .get()
        .and_then(|registry| registry.get(&account.provider))
        .ok_or_else(|| Error::Failed(format!("Unknown provider: {}", account.provider)))
}

/// Identity used to fill `${identity}` in a DAV endpoint's URI template.
pub(crate) fn account_identity(account: &Account) -> String {
    account
        .email
        .clone()
        .unwrap_or_else(|| account.username.clone())
}

pub(crate) async fn refresh_account_credentials(account: &mut Account) -> Result<()> {
    let mut auth = crate::auth::AuthManager::new()
        .await
        .map_err(|e| Error::Failed(format!("could not open the auth manager: {e}")))?;
    auth.ensure_credentials(account)
        .await
        .map_err(|e| Error::Failed(format!("could not refresh credentials: {e}")))
}

pub struct ServiceFactory;

impl ServiceFactory {
    pub fn create_services(account: &Account) -> Vec<Box<dyn AccountService>> {
        let mut services: Vec<Box<dyn AccountService>> = Vec::new();

        if let Some((_, value)) = account.services.get_key_value(&Service::Calendar)
            && *value
        {
            services.push(Box::new(CalendarService::new(account.clone())));
        }

        if account.services.get(&Service::Email) == Some(&true)
            && let Some(mail) = Self::create_service(account, &Service::Email)
        {
            services.push(mail);
        }

        if let Some((_, value)) = account.services.get_key_value(&Service::Tasks)
            && *value
        {
            services.push(Box::new(TasksService::new(account.clone())));
        }

        if let Some((_, value)) = account.services.get_key_value(&Service::Contacts)
            && *value
        {
            services.push(Box::new(ContactsService::new(account.clone())));
        }

        services
    }

    pub fn create_service(account: &Account, service: &Service) -> Option<Box<dyn AccountService>> {
        match service {
            Service::Calendar => Some(Box::new(CalendarService::new(account.clone()))),
            Service::Email if has_mail_endpoint(account) => {
                Some(Box::new(MailService::new(account.clone())))
            }
            Service::Email => None,
            Service::Tasks => Some(Box::new(TasksService::new(account.clone()))),
            Service::Contacts => Some(Box::new(ContactsService::new(account.clone()))),
        }
    }
}
