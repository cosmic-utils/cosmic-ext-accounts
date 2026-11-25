//! Reusable COSMIC widgets for the embedded account manager.
//!
//! The layout is built for the narrow context drawer used by Calendar, Mail,
//! and Tasks. It has three focused screens: an account overview, a provider
//! picker, and a browser sign-in wait state. Hosts keep the state and supply
//! localized labels; the library owns the layout and service filtering.

use std::borrow::Cow;

use cosmic::{
    Element,
    iced::{Alignment, Length},
    theme,
    widget::{self, text},
};

use crate::{
    Accounts, Error, Result,
    model::{AccountId, Provider, Service},
    oauth::AuthRequest,
    provider,
};

const GOOGLE_ICON: &[u8] = include_bytes!("../assets/google.png");
const MICROSOFT_ICON: &[u8] = include_bytes!("../assets/microsoft.png");
const MAIL_ICON: &[u8] = include_bytes!("../assets/mail.svg");
const CALENDAR_ICON: &[u8] = include_bytes!("../assets/calendar.svg");

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

/// Starts a browser sign-in and returns the request to complete later.
///
/// This deliberately does not open a browser: hosts store the pending request
/// first, then call [`open_sign_in_url`]. That lets the drawer show the
/// sign-in screen even when launching the browser fails, with an action to
/// retry opening it.
pub async fn begin_sign_in(
    accounts: &Accounts,
    provider: Provider,
    service: Service,
) -> Result<AuthRequest> {
    accounts.begin_auth(provider, service).await
}

/// Opens the authorization URL for a pending sign-in.
pub fn open_sign_in_url(request: &AuthRequest) -> Result<()> {
    open::that(&request.url)
        .map_err(|error| Error::AuthenticationFailed(format!("could not open browser: {error}")))
}

fn default_checking() -> String {
    "Checking accounts…".into()
}

fn default_finish_body(provider: Provider) -> String {
    format!(
        "We opened {} in your browser. Once you finish signing in there, your account shows up here.",
        provider_label(provider)
    )
}

fn default_sign_in(provider: Provider) -> String {
    format!("Sign in with {}", provider_label(provider))
}

/// User-facing strings for [`view`]. Hosts supply localized labels while the
/// library keeps responsibility for layout and service filtering.
pub struct Labels<'a> {
    pub title: Cow<'a, str>,
    pub checking: Cow<'a, str>,
    pub empty: Cow<'a, str>,
    pub no_providers: Cow<'a, str>,
    pub add: Cow<'a, str>,
    pub add_title: Cow<'a, str>,
    pub add_body: Cow<'a, str>,
    pub back: Cow<'a, str>,
    pub finish_title: Cow<'a, str>,
    pub finish_body: &'a dyn Fn(Provider) -> String,
    pub open_browser_again: Cow<'a, str>,
    pub cancel: Cow<'a, str>,
    pub reconnect: Cow<'a, str>,
    pub remove: Cow<'a, str>,
    pub sync: Cow<'a, str>,
    pub url: Cow<'a, str>,
    pub email: Cow<'a, str>,
    pub username: Cow<'a, str>,
    pub password: Cow<'a, str>,
    pub imap_host: Cow<'a, str>,
    pub imap_port: Cow<'a, str>,
    pub smtp_host: Cow<'a, str>,
    pub smtp_port: Cow<'a, str>,
    pub starttls: Cow<'a, str>,
    pub connect: Cow<'a, str>,
    pub sign_in: &'a dyn Fn(Provider) -> String,
}

impl<'a> Default for Labels<'a> {
    fn default() -> Self {
        Self {
            title: "Accounts".into(),
            checking: default_checking().into(),
            empty: "No accounts are connected.".into(),
            no_providers: "No providers are available in this build.".into(),
            add: "Add account".into(),
            add_title: "Add an account".into(),
            add_body: "Pick a provider. Sign in happens in your browser.".into(),
            back: "Back".into(),
            finish_title: "Finish signing in".into(),
            finish_body: &default_finish_body,
            open_browser_again: "Open browser again".into(),
            cancel: "Cancel".into(),
            reconnect: "Reconnect".into(),
            remove: "Remove".into(),
            sync: "Sync".into(),
            url: "Server URL".into(),
            email: "Email".into(),
            username: "Username".into(),
            password: "Password".into(),
            imap_host: "IMAP host".into(),
            imap_port: "IMAP port".into(),
            smtp_host: "SMTP host".into(),
            smtp_port: "SMTP port".into(),
            starttls: "SMTP STARTTLS".into(),
            connect: "Connect".into(),
            sign_in: &default_sign_in,
        }
    }
}

/// One account as it should appear in the host application.
///
/// Hosts pass their own rows because local enablement and sync status often
/// live in the application rather than in the embedded account manager.
pub struct AccountRow<'a> {
    pub id: AccountId,
    pub provider: Provider,
    pub label: Cow<'a, str>,
    pub detail: Cow<'a, str>,
    pub enabled: bool,
    pub status: Option<AccountStatus<'a>>,
}

/// A compact status badge shown under one account in the overview.
pub struct AccountStatus<'a> {
    pub icon: &'static str,
    pub text: Cow<'a, str>,
}

pub struct View<'a, M> {
    pub service: Service,
    pub accounts: Vec<AccountRow<'a>>,
    pub pending: Option<&'a AuthRequest>,
    pub loading: bool,
    /// Whether the provider picker should replace the overview.
    pub adding: bool,
    /// Providers to offer. `None` uses every provider enabled for `service`.
    /// Hosts that keep an app-local editor can narrow this list.
    pub providers: Option<&'a [Provider]>,
    /// Provider-specific form to show instead of the other screens.
    pub form: Option<Form<'a, M>>,
    /// Host-owned content appended to the overview, such as local accounts.
    pub footer: Option<Element<'a, M>>,
    pub labels: Labels<'a>,
    pub on_add_open: &'a dyn Fn() -> M,
    pub on_add_close: &'a dyn Fn() -> M,
    pub on_open_url: &'a dyn Fn() -> M,
    pub on_add: &'a dyn Fn(Provider) -> M,
    pub on_cancel: &'a dyn Fn(&str) -> M,
    pub on_remove: &'a dyn Fn(AccountId) -> M,
    pub on_reconnect: &'a dyn Fn(AccountId) -> M,
    pub on_toggle: &'a dyn Fn(AccountId, bool) -> M,
    /// Optional per-account sync action. Hosts without background sync pass
    /// `None`, which hides the button.
    pub on_sync: Option<&'a dyn Fn(AccountId) -> M>,
}

struct OverviewActions<'a, M> {
    on_add_open: &'a dyn Fn() -> M,
    on_remove: &'a dyn Fn(AccountId) -> M,
    on_reconnect: &'a dyn Fn(AccountId) -> M,
    on_toggle: &'a dyn Fn(AccountId, bool) -> M,
    on_sync: Option<&'a dyn Fn(AccountId) -> M>,
}

/// Builds the account page for one service at context-drawer width.
pub fn view<'a, M>(view: View<'a, M>) -> Element<'a, M>
where
    M: Clone + 'static,
{
    if let Some(form) = view.form {
        return form_page(form, &view.labels);
    }

    if let Some(request) = view.pending {
        return signing_in_page(request, &view.labels, view.on_cancel, view.on_open_url);
    }

    let providers = match view.providers {
        Some(providers) => providers.to_vec(),
        None => provider::providers_for(view.service),
    };

    if view.adding {
        return add_page(providers, &view.labels, view.on_add, view.on_add_close);
    }

    overview(
        view.accounts,
        view.loading,
        providers.is_empty(),
        &view.labels,
        view.footer,
        OverviewActions {
            on_add_open: view.on_add_open,
            on_remove: view.on_remove,
            on_reconnect: view.on_reconnect,
            on_toggle: view.on_toggle,
            on_sync: view.on_sync,
        },
    )
}

fn overview<'a, M>(
    accounts: Vec<AccountRow<'a>>,
    loading: bool,
    no_providers: bool,
    labels: &Labels<'a>,
    footer: Option<Element<'a, M>>,
    actions: OverviewActions<'a, M>,
) -> Element<'a, M>
where
    M: Clone + 'static,
{
    let spacing = theme::spacing();
    let mut column = widget::column::with_capacity(5)
        .spacing(spacing.space_m)
        .width(Length::Fill);

    if loading {
        column = column.push(text::body(labels.checking.clone()));
    } else if accounts.is_empty() {
        column = column.push(empty_state(labels.empty.clone()));
    }

    for row in &accounts {
        column = column.push(account_card(
            row,
            labels,
            actions.on_remove,
            actions.on_reconnect,
            actions.on_toggle,
            actions.on_sync,
        ));
    }

    if no_providers {
        column = column.push(text::body(labels.no_providers.clone()));
    } else {
        column = column.push(
            widget::button::standard(labels.add.clone())
                .leading_icon(widget::icon::from_name("list-add-symbolic").size(16))
                .width(Length::Fill)
                .on_press((actions.on_add_open)()),
        );
    }

    if let Some(footer) = footer {
        column = column.push(footer);
    }

    column.into()
}

fn add_page<'a, M>(
    providers: Vec<Provider>,
    labels: &Labels<'a>,
    on_add: &'a dyn Fn(Provider) -> M,
    on_back: &'a dyn Fn() -> M,
) -> Element<'a, M>
where
    M: Clone + 'static,
{
    let spacing = theme::spacing();
    let mut column = widget::column::with_capacity(3)
        .spacing(spacing.space_m)
        .width(Length::Fill);

    column = column.push(widget::text::title3(labels.add_title.clone()));
    column = column.push(text::body(labels.add_body.clone()));

    let mut rows = widget::column::with_capacity(providers.len()).spacing(spacing.space_xs);
    for provider in providers {
        rows = rows.push(provider_row(
            provider_icon(provider, 32),
            (labels.sign_in)(provider),
            on_add(provider),
        ));
    }
    column = column.push(rows);
    column = column.push(
        widget::button::standard(labels.back.clone())
            .width(Length::Fill)
            .on_press(on_back()),
    );

    column.into()
}

fn signing_in_page<'a, M>(
    request: &AuthRequest,
    labels: &Labels<'a>,
    on_cancel: &'a dyn Fn(&str) -> M,
    on_open_url: &'a dyn Fn() -> M,
) -> Element<'a, M>
where
    M: Clone + 'static,
{
    let spacing = theme::spacing();
    let state = request.state.clone();

    widget::column::with_capacity(5)
        .spacing(spacing.space_m)
        .width(Length::Fill)
        .push(
            widget::column::with_capacity(3)
                .spacing(spacing.space_s)
                .align_x(Alignment::Center)
                .push(provider_icon(request.provider, 48))
                .push(widget::text::title3(labels.finish_title.clone()))
                .push(
                    text::body((labels.finish_body)(request.provider))
                        .width(Length::Fill)
                        .align_x(Alignment::Center),
                ),
        )
        .push(widget::progress_bar::indeterminate_linear().width(Length::Fill))
        .push(
            widget::button::standard(labels.open_browser_again.clone())
                .width(Length::Fill)
                .on_press(on_open_url()),
        )
        .push(
            widget::button::standard(labels.cancel.clone())
                .width(Length::Fill)
                .on_press(on_cancel(&state)),
        )
        .into()
}

fn account_card<'a, M>(
    row: &AccountRow<'a>,
    labels: &Labels<'a>,
    on_remove: &'a dyn Fn(AccountId) -> M,
    on_reconnect: &'a dyn Fn(AccountId) -> M,
    on_toggle: &'a dyn Fn(AccountId, bool) -> M,
    on_sync: Option<&'a dyn Fn(AccountId) -> M>,
) -> Element<'a, M>
where
    M: Clone + 'static,
{
    let spacing = theme::spacing();
    let id = row.id;
    let mut details = widget::column::with_capacity(2)
        .spacing(spacing.space_xxxs)
        .push(text::body(row.label.to_string()).width(Length::Fill))
        .push(text::caption(row.detail.to_string()));

    if let Some(status) = &row.status {
        details = details.push(
            widget::row::with_capacity(2)
                .spacing(spacing.space_xxxs)
                .push(widget::icon::from_name(status.icon).size(14))
                .push(text::caption(status.text.to_string())),
        );
    }

    widget::container(
        widget::column::with_capacity(3)
            .spacing(spacing.space_s)
            .push(
                widget::row::with_capacity(3)
                    .spacing(spacing.space_s)
                    .align_y(Alignment::Center)
                    .push(provider_icon(row.provider, 28))
                    .push(details)
                    .push(
                        widget::toggler(row.enabled)
                            .on_toggle(move |enabled| on_toggle(id, enabled)),
                    ),
            )
            .push(
                widget::row::with_capacity(3)
                    .spacing(spacing.space_xs)
                    .push_maybe(on_sync.map(|on_sync| {
                        widget::button::text(labels.sync.clone())
                            .width(Length::FillPortion(1))
                            .on_press(on_sync(id))
                    }))
                    .push(
                        widget::button::text(labels.reconnect.clone())
                            .width(Length::FillPortion(1))
                            .on_press(on_reconnect(id)),
                    )
                    .push(
                        widget::button::destructive(labels.remove.clone())
                            .width(Length::FillPortion(1))
                            .on_press(on_remove(id)),
                    ),
            ),
    )
    .class(theme::Container::Card)
    .padding(spacing.space_s)
    .width(Length::Fill)
    .into()
}

fn provider_row<'a, M>(icon: Element<'a, M>, label: String, message: M) -> Element<'a, M>
where
    M: Clone + 'static,
{
    let spacing = theme::spacing();
    widget::button::custom(
        widget::row::with_capacity(3)
            .spacing(spacing.space_s)
            .align_y(Alignment::Center)
            .push(icon)
            .push(text::body(label.clone()).width(Length::Fill))
            .push(widget::icon::from_name("go-next-symbolic").size(16)),
    )
    .padding([spacing.space_s, spacing.space_m])
    .width(Length::Fill)
    .name(label)
    .on_press(message)
    .into()
}

fn provider_icon<'a, M>(provider: Provider, size: u16) -> Element<'a, M>
where
    M: Clone + 'static,
{
    let handle = match provider {
        Provider::Google => widget::icon::from_raster_bytes(GOOGLE_ICON),
        Provider::Microsoft => widget::icon::from_raster_bytes(MICROSOFT_ICON),
        Provider::CustomCalDav => widget::icon::from_svg_bytes(CALENDAR_ICON),
        Provider::CustomImapSmtp => widget::icon::from_svg_bytes(MAIL_ICON),
        Provider::CustomCardDav | Provider::CustomLdap => {
            widget::icon::from_name("x-office-address-book-symbolic").into()
        }
    };
    widget::icon(handle)
        .size(size)
        .width(Length::Fixed(f32::from(size)))
        .height(Length::Fixed(f32::from(size)))
        .into()
}

fn empty_state<'a, M>(message: Cow<'a, str>) -> Element<'a, M>
where
    M: Clone + 'static,
{
    widget::container(
        widget::column::with_capacity(2)
            .spacing(theme::spacing().space_s)
            .align_x(Alignment::Center)
            .push(widget::icon::from_name("system-users-symbolic").size(40))
            .push(text::body(message).align_x(Alignment::Center)),
    )
    .center_x(Length::Fill)
    .padding(theme::spacing().space_m)
    .into()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormField {
    Url,
    Email,
    Username,
    Password,
    ImapHost,
    ImapPort,
    SmtpHost,
    SmtpPort,
    BaseDn,
}

/// A host-bound custom account form.
pub struct Form<'a, M> {
    pub state: FormState<'a>,
    pub on_change: &'a dyn Fn(FormField, String) -> M,
    pub on_starttls: Option<&'a dyn Fn(bool) -> M>,
    pub on_submit: &'a dyn Fn() -> M,
    pub on_cancel: &'a dyn Fn() -> M,
}

pub struct FormState<'a> {
    pub provider: Provider,
    pub service: Service,
    pub url: &'a str,
    pub email: &'a str,
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
    label: Cow<'a, str>,
    placeholder: &'static str,
    value: &'a str,
    field: FormField,
    on_change: &'a dyn Fn(FormField, String) -> M,
) -> Element<'a, M>
where
    M: Clone + 'static,
{
    widget::column::with_capacity(2)
        .spacing(theme::spacing().space_xxxs)
        .push(text::caption(label))
        .push(
            widget::text_input(placeholder, value)
                .width(Length::Fill)
                .on_input(move |value| on_change(field, value)),
        )
        .into()
}

fn secure_row<'a, M>(
    label: Cow<'a, str>,
    value: &'a str,
    on_change: &'a dyn Fn(FormField, String) -> M,
) -> Element<'a, M>
where
    M: Clone + 'static,
{
    widget::column::with_capacity(2)
        .spacing(theme::spacing().space_xxxs)
        .push(text::caption(label))
        .push(
            widget::text_input::secure_input("", value, None, true)
                .width(Length::Fill)
                .on_input(move |value| on_change(FormField::Password, value)),
        )
        .into()
}

/// Builds the provider-specific form for a custom account at drawer width.
pub fn form_page<'a, M>(form: Form<'a, M>, labels: &Labels<'a>) -> Element<'a, M>
where
    M: Clone + 'static,
{
    let Form {
        state,
        on_change,
        on_starttls,
        on_submit,
        on_cancel,
    } = form;
    let spacing = theme::spacing();
    let mut column = widget::column::with_capacity(12)
        .spacing(spacing.space_s)
        .width(Length::Fill)
        .push(widget::text::title3(provider_label(state.provider)));

    match state.provider {
        Provider::CustomCalDav | Provider::CustomCardDav => {
            column = column
                .push(input_row(
                    labels.url.clone(),
                    "https://example.com/dav",
                    state.url,
                    FormField::Url,
                    on_change,
                ))
                .push(input_row(
                    labels.username.clone(),
                    "Username",
                    state.username,
                    FormField::Username,
                    on_change,
                ))
                .push(secure_row(
                    labels.password.clone(),
                    state.password,
                    on_change,
                ));
        }
        Provider::CustomImapSmtp => {
            column = column
                .push(input_row(
                    labels.email.clone(),
                    "user@example.com",
                    state.email,
                    FormField::Email,
                    on_change,
                ))
                .push(input_row(
                    labels.username.clone(),
                    "user@example.com",
                    state.username,
                    FormField::Username,
                    on_change,
                ))
                .push(secure_row(
                    labels.password.clone(),
                    state.password,
                    on_change,
                ))
                .push(input_row(
                    labels.imap_host.clone(),
                    "imap.example.com",
                    state.imap_host,
                    FormField::ImapHost,
                    on_change,
                ))
                .push(input_row(
                    labels.imap_port.clone(),
                    "993",
                    state.imap_port,
                    FormField::ImapPort,
                    on_change,
                ))
                .push(input_row(
                    labels.smtp_host.clone(),
                    "smtp.example.com",
                    state.smtp_host,
                    FormField::SmtpHost,
                    on_change,
                ))
                .push(input_row(
                    labels.smtp_port.clone(),
                    "587",
                    state.smtp_port,
                    FormField::SmtpPort,
                    on_change,
                ))
                .push_maybe(on_starttls.map(|on_toggle| {
                    widget::row::with_capacity(2)
                        .spacing(spacing.space_s)
                        .align_y(Alignment::Center)
                        .push(text::body(labels.starttls.clone()).width(Length::Fill))
                        .push(widget::toggler(state.starttls).on_toggle(on_toggle))
                }));
        }
        Provider::CustomLdap => {
            column = column
                .push(input_row(
                    labels.url.clone(),
                    "ldaps://example.com",
                    state.url,
                    FormField::Url,
                    on_change,
                ))
                .push(input_row(
                    "Base DN".into(),
                    "dc=example,dc=com",
                    state.base_dn,
                    FormField::BaseDn,
                    on_change,
                ))
                .push(input_row(
                    labels.username.clone(),
                    "Username",
                    state.username,
                    FormField::Username,
                    on_change,
                ))
                .push(secure_row(
                    labels.password.clone(),
                    state.password,
                    on_change,
                ));
        }
        Provider::Google | Provider::Microsoft => {
            column = column.push(text::body((labels.finish_body)(state.provider)));
        }
    }

    column
        .push(
            widget::row::with_capacity(2)
                .spacing(spacing.space_xs)
                .push(
                    widget::button::standard(labels.cancel.clone())
                        .width(Length::FillPortion(1))
                        .on_press(on_cancel()),
                )
                .push(
                    widget::button::suggested(labels.connect.clone())
                        .width(Length::FillPortion(1))
                        .on_press(on_submit()),
                ),
        )
        .into()
}
