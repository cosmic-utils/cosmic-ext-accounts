//! Reusable COSMIC settings widgets for the embedded account manager.
//!
//! The panel is service-scoped: a host that enables several service features
//! still sees only the providers and forms valid for the service it passes in.

use cosmic::{
    iced::Alignment,
    widget::{self, text},
    Element,
};

use crate::{
    Accounts, Error, Result,
    model::{AccountId, Provider, Service},
    oauth::AuthRequest,
    provider,
};

pub fn provider_label(provider: Provider) -> &'static str {
    match provider {
        Provider::Google => "Google",
        Provider::Microsoft => "Microsoft",
        Provider::CustomCalDav => "CalDAV",
        Provider::CustomImapSmtp => "IMAP/SMTP",
        Provider::CustomCardDav => "CardDAV",
        Provider::CustomLdap => "LDAP",
    }
}

fn service_label(service: Service) -> &'static str {
    match service {
        Service::Mail => "Mail",
        Service::Calendar => "Calendar",
        Service::Contacts => "Contacts",
        Service::Tasks => "Tasks",
    }
}

fn account_label(account: &crate::model::Account) -> String {
    account
        .email_or_identity()
        .to_owned()
}

/// Opens the sign-in URL in the user's browser and returns the request that
/// must later be passed to [`Accounts::complete_auth`].
pub async fn sign_in(
    accounts: &Accounts,
    provider: Provider,
    service: Service,
) -> Result<AuthRequest> {
    let request = accounts.begin_auth(provider, service).await?;
    open::that(&request.url)
        .map_err(|error| Error::AuthenticationFailed(format!("could not open browser: {error}")))?;
    Ok(request)
}

pub struct View<'a, M> {
    pub service: Service,
    pub accounts: &'a [crate::model::Account],
    pub pending: Option<&'a AuthRequest>,
    pub error: Option<&'a str>,
    pub on_add: &'a dyn Fn(Provider) -> M,
    pub on_cancel: &'a dyn Fn(&str) -> M,
    pub on_remove: &'a dyn Fn(AccountId) -> M,
    pub on_reconnect: &'a dyn Fn(AccountId) -> M,
    pub on_toggle: &'a dyn Fn(AccountId, bool) -> M,
}

/// Builds the account list and sign-in controls for one service.
pub fn view<'a, M>(view: View<'a, M>) -> Element<'a, M>
where
    M: Clone + 'static,
{
    let mut section = widget::settings::section().title("Accounts");

    if let Some(error) = view.error {
        section = section.add(text::body(error));
    }

    if let Some(request) = view.pending {
        let state = request.state.clone();
        section = section.add(
            widget::settings::item::builder(format!(
                "Waiting for {} sign-in",
                provider_label(request.provider)
            ))
            .description("Finish the sign-in in your browser.")
            .control(
                widget::button::standard("Cancel")
                    .on_press((view.on_cancel)(&state)),
            ),
        );
    }

    if view.accounts.is_empty() && view.pending.is_none() {
        section = section.add(text::body(format!(
            "No {} accounts are connected.",
            service_label(view.service)
        )));
    }

    for account in view.accounts {
        let id = account.id;
        let controls = widget::row::with_capacity(3)
            .spacing(cosmic::theme::spacing().space_xs)
            .align_y(Alignment::Center)
            .push(widget::button::standard("Reconnect").on_press((view.on_reconnect)(id)))
            .push(widget::button::destructive("Remove").on_press((view.on_remove)(id)))
            .push(widget::toggler(account.enabled).on_toggle(move |enabled| {
                (view.on_toggle)(id, enabled)
            }));
        section = section.add(
            widget::settings::item::builder(account_label(account))
                .description(format!(
                    "{} · {}",
                    provider_label(account.provider),
                    service_label(account.service)
                ))
                .control(controls),
        );
    }

    let providers = provider::providers_for(view.service);
    if providers.is_empty() {
        section = section.add(text::body(format!(
            "No providers are available for {} in this build.",
            service_label(view.service)
        )));
    } else {
        let mut buttons = widget::row::with_capacity(providers.len())
            .spacing(cosmic::theme::spacing().space_xs);
        for provider in providers {
            buttons = buttons.push(
                widget::button::standard(format!("Sign in with {}", provider_label(provider)))
                    .on_press((view.on_add)(provider)),
            );
        }
        section = section.add(
            widget::settings::item::builder("Add account")
                .description(format!("Connect a {} account.", service_label(view.service)))
                .control(buttons),
        );
    }

    section.into()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormField {
    Url,
    Username,
    Password,
    ImapHost,
    ImapPort,
    SmtpHost,
    SmtpPort,
    BaseDn,
}

pub struct FormState<'a> {
    pub provider: Provider,
    pub service: Service,
    pub url: &'a str,
    pub username: &'a str,
    pub password: &'a str,
    pub imap_host: &'a str,
    pub imap_port: &'a str,
    pub smtp_host: &'a str,
    pub smtp_port: &'a str,
    pub base_dn: &'a str,
    pub starttls: bool,
}

fn input_row<'a, M>(
    label: &'static str,
    placeholder: &'static str,
    value: &'a str,
    field: FormField,
    on_change: &'a dyn Fn(FormField, String) -> M,
) -> Element<'a, M>
where
    M: Clone + 'static,
{
    widget::settings::item::builder(label)
        .control(widget::text_input(placeholder, value).on_input(move |value| {
            on_change(field, value)
        }))
        .into()
}

fn secure_row<'a, M>(
    value: &'a str,
    on_change: &'a dyn Fn(FormField, String) -> M,
) -> Element<'a, M>
where
    M: Clone + 'static,
{
    widget::settings::item::builder("Password")
        .control(
            widget::text_input::secure_input("Password", value, None, true)
                .on_input(move |value| on_change(FormField::Password, value)),
        )
        .into()
}

/// Builds the provider-specific form for a custom account.
pub fn form_view<'a, M>(
    state: FormState<'a>,
    on_change: &'a dyn Fn(FormField, String) -> M,
    on_starttls: &'a dyn Fn(bool) -> M,
    on_submit: &'a dyn Fn() -> M,
    on_cancel: &'a dyn Fn() -> M,
) -> Element<'a, M>
where
    M: Clone + 'static,
{
    let mut column = widget::column::with_capacity(12).spacing(8);
    let service = service_label(state.service);

    match state.provider {
        Provider::CustomCalDav | Provider::CustomCardDav => {
            column = column
                .push(input_row(
                    "Server URL",
                    "https://example.com/dav",
                    state.url,
                    FormField::Url,
                    on_change,
                ))
                .push(input_row(
                    "Username",
                    "Username",
                    state.username,
                    FormField::Username,
                    on_change,
                ))
                .push(secure_row(state.password, on_change));
        }
        Provider::CustomImapSmtp => {
            column = column
                .push(input_row(
                    "Email",
                    "user@example.com",
                    state.username,
                    FormField::Username,
                    on_change,
                ))
                .push(secure_row(state.password, on_change))
                .push(input_row(
                    "IMAP host",
                    "imap.example.com",
                    state.imap_host,
                    FormField::ImapHost,
                    on_change,
                ))
                .push(input_row(
                    "IMAP port",
                    "993",
                    state.imap_port,
                    FormField::ImapPort,
                    on_change,
                ))
                .push(input_row(
                    "SMTP host",
                    "smtp.example.com",
                    state.smtp_host,
                    FormField::SmtpHost,
                    on_change,
                ))
                .push(input_row(
                    "SMTP port",
                    "587",
                    state.smtp_port,
                    FormField::SmtpPort,
                    on_change,
                ))
                .push(
                    widget::settings::item::builder("SMTP STARTTLS")
                        .control(widget::toggler(state.starttls).on_toggle(on_starttls)),
                );
        }
        Provider::CustomLdap => {
            column = column
                .push(input_row(
                    "Server URL",
                    "ldaps://example.com",
                    state.url,
                    FormField::Url,
                    on_change,
                ))
                .push(input_row(
                    "Base DN",
                    "dc=example,dc=com",
                    state.base_dn,
                    FormField::BaseDn,
                    on_change,
                ))
                .push(input_row(
                    "Username",
                    "Username",
                    state.username,
                    FormField::Username,
                    on_change,
                ))
                .push(secure_row(state.password, on_change));
        }
        Provider::Google | Provider::Microsoft => {
            column = column.push(text::body(format!(
                "{} uses browser sign-in; no custom form is needed.",
                provider_label(state.provider)
            )));
        }
    }

    let _ = service;

    column
        .push(
            widget::row::with_capacity(2)
                .spacing(cosmic::theme::spacing().space_xs)
                .push(widget::button::standard("Cancel").on_press((on_cancel)()))
                .push(widget::button::suggested(format!("Connect {service}")).on_press((on_submit)())),
        )
        .into()
}
