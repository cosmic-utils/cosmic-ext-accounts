//! Bundled provider metadata.
//!
//! Client IDs and secrets are intentionally *not* stored here. Applications
//! supply their own registrations through [`crate::config::ClientConfig`],
//! keyed by provider and service.

use crate::{
    error::Error,
    model::{Provider, Resource, Service},
};

#[derive(Clone, Copy)]
pub struct OAuthSpec {
    pub auth_url: &'static str,
    pub token_url: &'static str,
    pub userinfo_url: &'static str,
    pub display_name_field: &'static str,
    pub email_field: &'static str,
    pub username_field: Option<&'static str>,
    pub extra_params: &'static [(&'static str, &'static str)],
}

#[derive(Clone, Copy)]
pub struct ProviderSpec {
    pub provider: Provider,
    pub name: &'static str,
    pub oauth: Option<OAuthSpec>,
    pub default_redirect_uri: &'static str,
}

const GOOGLE: ProviderSpec = ProviderSpec {
    provider: Provider::Google,
    name: "Google",
    oauth: Some(OAuthSpec {
        auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
        token_url: "https://oauth2.googleapis.com/token",
        userinfo_url: "https://www.googleapis.com/oauth2/v2/userinfo",
        display_name_field: "name",
        email_field: "email",
        username_field: None,
        extra_params: &[("access_type", "offline"), ("prompt", "consent")],
    }),
    default_redirect_uri: "http://127.0.0.1/callback",
};

const MICROSOFT: ProviderSpec = ProviderSpec {
    provider: Provider::Microsoft,
    name: "Microsoft",
    oauth: Some(OAuthSpec {
        auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
        token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
        userinfo_url: "https://graph.microsoft.com/v1.0/me",
        display_name_field: "displayName",
        email_field: "mail",
        username_field: Some("userPrincipalName"),
        extra_params: &[],
    }),
    default_redirect_uri: "http://127.0.0.1/callback",
};

const CUSTOM_CALDAV: ProviderSpec = ProviderSpec {
    provider: Provider::CustomCalDav,
    name: "CalDAV",
    oauth: None,
    default_redirect_uri: "http://127.0.0.1/callback",
};

const CUSTOM_MAIL: ProviderSpec = ProviderSpec {
    provider: Provider::CustomImapSmtp,
    name: "IMAP/SMTP",
    oauth: None,
    default_redirect_uri: "http://127.0.0.1/callback",
};

const CUSTOM_CARDDAV: ProviderSpec = ProviderSpec {
    provider: Provider::CustomCardDav,
    name: "CardDAV",
    oauth: None,
    default_redirect_uri: "http://127.0.0.1/callback",
};

const CUSTOM_LDAP: ProviderSpec = ProviderSpec {
    provider: Provider::CustomLdap,
    name: "LDAP",
    oauth: None,
    default_redirect_uri: "http://127.0.0.1/callback",
};

pub fn spec(provider: Provider) -> Option<&'static ProviderSpec> {
    match provider {
        Provider::Google => Some(&GOOGLE),
        Provider::Microsoft => Some(&MICROSOFT),
        Provider::CustomCalDav => Some(&CUSTOM_CALDAV),
        Provider::CustomImapSmtp => Some(&CUSTOM_MAIL),
        Provider::CustomCardDav => Some(&CUSTOM_CARDDAV),
        Provider::CustomLdap => Some(&CUSTOM_LDAP),
    }
}

/// Providers offered for a service in this build.
///
/// A binary built with several service features still sees only the providers
/// valid for the service passed to its panel.
pub fn providers_for(service: Service) -> Vec<Provider> {
    match service {
        Service::Calendar => {
            #[cfg(feature = "calendar")]
            {
                vec![
                    Provider::Google,
                    Provider::Microsoft,
                    Provider::CustomCalDav,
                ]
            }
            #[cfg(not(feature = "calendar"))]
            {
                let _ = service;
                Vec::new()
            }
        }
        Service::Mail => {
            #[cfg(feature = "mail")]
            {
                vec![
                    Provider::Google,
                    Provider::Microsoft,
                    Provider::CustomImapSmtp,
                ]
            }
            #[cfg(not(feature = "mail"))]
            {
                let _ = service;
                Vec::new()
            }
        }
        Service::Contacts => {
            #[cfg(feature = "contacts")]
            {
                vec![
                    Provider::Google,
                    Provider::Microsoft,
                    Provider::CustomCardDav,
                    Provider::CustomLdap,
                ]
            }
            #[cfg(not(feature = "contacts"))]
            {
                let _ = service;
                Vec::new()
            }
        }
        Service::Tasks => {
            #[cfg(feature = "tasks")]
            {
                vec![Provider::Google, Provider::Microsoft]
            }
            #[cfg(not(feature = "tasks"))]
            {
                let _ = service;
                Vec::new()
            }
        }
    }
}

pub fn supports(provider: Provider, service: Service) -> bool {
    providers_for(service).contains(&provider)
}

const fn service_enabled(service: Service) -> bool {
    match service {
        Service::Calendar => cfg!(feature = "calendar"),
        Service::Mail => cfg!(feature = "mail"),
        Service::Contacts => cfg!(feature = "contacts"),
        Service::Tasks => cfg!(feature = "tasks"),
    }
}

pub fn ensure_supported(provider: Provider, service: Service) -> Result<(), Error> {
    if !service_enabled(service) {
        return Err(Error::FeatureDisabled(match service {
            Service::Calendar => "calendar",
            Service::Mail => "mail",
            Service::Contacts => "contacts",
            Service::Tasks => "tasks",
        }));
    }
    if !supports(provider, service) {
        return Err(Error::UnsupportedProvider(provider, service));
    }
    Ok(())
}

/// The resource used for the token returned by the authorization exchange.
pub fn primary_resource(provider: Provider, service: Service) -> Resource {
    match (provider, service) {
        (Provider::Microsoft, Service::Calendar | Service::Contacts | Service::Tasks) => {
            Resource::Graph
        }
        (Provider::Microsoft, Service::Mail) => Resource::Graph,
        (Provider::Google, _) => Resource::Primary,
        (Provider::CustomCalDav, _) => Resource::CalDav,
        (Provider::CustomImapSmtp, _) => Resource::ImapSmtp,
        (Provider::CustomCardDav, _) => Resource::CardDav,
        (Provider::CustomLdap, _) => Resource::Ldap,
    }
}

fn base_scopes(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::Google => &["openid", "email", "profile"],
        Provider::Microsoft => &["offline_access", "openid", "email", "profile"],
        _ => &[],
    }
}

fn service_scopes(service: Service, resource: Resource) -> &'static [&'static str] {
    match (service, resource) {
        (Service::Calendar, Resource::Graph) => &["https://graph.microsoft.com/calendars.read"],
        (Service::Calendar, Resource::Primary) => {
            &["https://www.googleapis.com/auth/calendar"]
        }
        (Service::Mail, Resource::Graph) => &[
            "https://graph.microsoft.com/Mail.ReadWrite",
            "https://graph.microsoft.com/Mail.Send",
        ],
        (Service::Mail, Resource::Exchange) => &[
            "https://outlook.office.com/IMAP.AccessAsUser.All",
            "https://outlook.office.com/SMTP.Send",
        ],
        (Service::Mail, Resource::Primary) => &["https://mail.google.com/"],
        (Service::Contacts, Resource::Graph) => {
            &["https://graph.microsoft.com/contacts.read"]
        }
        (Service::Contacts, Resource::Primary) => {
            &["https://www.googleapis.com/auth/contacts"]
        }
        (Service::Tasks, Resource::Graph) => &[
            "https://graph.microsoft.com/tasks.readwrite",
            "https://graph.microsoft.com/tasks.readwrite.shared",
        ],
        (Service::Tasks, Resource::Primary) => &["https://www.googleapis.com/auth/tasks"],
        _ => &[],
    }
}

/// All resources a service needs for a provider.
pub fn resources_for(provider: Provider, service: Service) -> Vec<Resource> {
    match (provider, service) {
        (Provider::Microsoft, Service::Mail) => vec![Resource::Graph, Resource::Exchange],
        _ => vec![primary_resource(provider, service)],
    }
}

/// Scopes requested during the authorization redirect.
pub fn authorization_scopes(provider: Provider, service: Service) -> Vec<String> {
    let mut scopes: Vec<String> = base_scopes(provider)
        .iter()
        .map(|scope| (*scope).to_owned())
        .collect();
    for resource in resources_for(provider, service) {
        scopes.extend(service_scopes(service, resource).iter().map(|s| (*s).to_owned()));
    }
    scopes.sort();
    scopes.dedup();
    scopes
}

/// Scopes used when refreshing a particular resource.
pub fn resource_scopes(
    provider: Provider,
    service: Service,
    resource: Resource,
) -> Vec<String> {
    let mut scopes: Vec<String> = base_scopes(provider)
        .iter()
        .map(|scope| (*scope).to_owned())
        .collect();
    scopes.extend(
        service_scopes(service, resource)
            .iter()
            .map(|scope| (*scope).to_owned()),
    );
    scopes.sort();
    scopes.dedup();
    scopes
}

/// Provider-supplied DAV/Graph base URL where applicable.
pub fn endpoint_base(provider: Provider, service: Service) -> Option<&'static str> {
    match (provider, service) {
        (Provider::Google, Service::Calendar) => {
            Some("https://apidata.googleusercontent.com/caldav/v2/")
        }
        (Provider::Google, Service::Contacts) => {
            Some("https://www.googleapis.com/.well-known/carddav")
        }
        (Provider::Google, Service::Tasks) => Some("https://tasks.googleapis.com/tasks/v1/"),
        (Provider::Microsoft, Service::Calendar | Service::Contacts) => {
            Some("https://outlook.office365.com/")
        }
        (Provider::Microsoft, Service::Tasks) => Some("https://graph.microsoft.com/v1.0/me/todo"),
        _ => None,
    }
}
