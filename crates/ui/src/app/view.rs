// SPDX-License-Identifier: GPL-3.0-only

use crate::app::{AppModel, Message};
use crate::fl;
use accounts_core::Local;
use accounts_core::models::{
    Account, CUSTOM_CALDAV_PROVIDER, CUSTOM_MAIL_PROVIDER, IconSource, Service,
};
use chrono::{DateTime, Utc};
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::{Alignment, Length};
use cosmic::prelude::*;
use cosmic::theme::spacing;
use cosmic::widget;

const APP_ICON: &[u8] = include_bytes!("../../resources/icons/hicolor/scalable/apps/icon.svg");
const GOOGLE_ICON: &[u8] = include_bytes!("../../resources/img/google.png");
const MICROSOFT_ICON: &[u8] = include_bytes!("../../resources/img/microsoft.png");
const MAIL_ICON: &[u8] = include_bytes!("../../resources/img/mail.svg");
const CALENDAR_ICON: &[u8] = include_bytes!("../../resources/img/calendar.svg");

const CONTENT_WIDTH: f32 = 720.0;
const PROSE_WIDTH: f32 = 520.0;
const PICKER_WIDTH: f32 = 360.0;

impl AppModel {
    pub(super) fn welcome_view(&self) -> Element<'_, Message> {
        if self.client.is_none() {
            return self.empty_state(
                "dialog-warning-symbolic",
                fl!("no-account-providers"),
                fl!("no-account-providers-body"),
                Some(
                    widget::button::suggested(fl!("try-again"))
                        .on_press(Message::CreateClient)
                        .into(),
                ),
            );
        }

        let header = widget::Column::new()
            .push(
                widget::icon(widget::icon::from_svg_bytes(APP_ICON))
                    .size(96)
                    .width(Length::Fixed(96.0))
                    .height(Length::Fixed(96.0)),
            )
            .push(widget::text::title1(fl!("app-title")).align_x(Horizontal::Center))
            .push(
                widget::text::body(fl!("connect-accounts"))
                    .align_x(Horizontal::Center)
                    .width(Length::Fixed(PROSE_WIDTH)),
            )
            .align_x(Alignment::Center)
            .spacing(spacing().space_s);

        widget::Column::new()
            .push(header)
            .push(
                self.provider_list()
                    .apply(widget::container)
                    .max_width(PICKER_WIDTH),
            )
            .align_x(Alignment::Center)
            .spacing(spacing().space_l)
            .apply(widget::container)
            .center(Length::Fill)
            .padding(spacing().space_l)
            .into()
    }

    /// Shown when accounts exist but none is selected.
    pub(super) fn no_selection_view(&self) -> Element<'_, Message> {
        self.empty_state(
            "system-users-symbolic",
            fl!("no-account-selected"),
            fl!("no-account-selected-body"),
            None,
        )
    }

    pub(super) fn account_view<'a>(&'a self, account: &'a Account) -> Element<'a, Message> {
        let mut sections = vec![
            self.account_header(account),
            self.account_section(account),
            self.services_section(account),
            self.grants_section(),
            self.details_section(account),
        ];

        if !account.enabled {
            sections.insert(1, warning_banner(fl!("account-disabled-warning")));
        }

        widget::settings::view_column(sections)
            .apply(widget::container)
            .max_width(CONTENT_WIDTH)
            .padding(spacing().space_m)
            .apply(widget::container)
            .center_x(Length::Fill)
            .apply(widget::scrollable)
            .height(Length::Fill)
            .into()
    }

    fn account_header<'a>(&'a self, account: &'a Account) -> Element<'a, Message> {
        let label = self.account_label(account);
        let title = if account.display_name.is_empty() {
            label.clone()
        } else {
            account.display_name.clone()
        };

        let mut names = widget::Column::new()
            .push(widget::text::title3(title.clone()))
            .spacing(spacing().space_xxxs);

        if label != title {
            names = names.push(widget::text::body(label).class(cosmic::style::Text::Default));
        }

        widget::Row::new()
            .push(self.provider_icon(&account.provider, 56))
            .push(names)
            .spacing(spacing().space_s)
            .align_y(Alignment::Center)
            .into()
    }

    fn account_section<'a>(&self, account: &'a Account) -> Element<'a, Message> {
        let (label, action, message) = if account.provider == CUSTOM_CALDAV_PROVIDER {
            (
                fl!("caldav-update-password"),
                fl!("caldav-update-password"),
                Message::OpenDialog(super::DialogPage::CalDav(Some(account.id))),
            )
        } else if account.provider == CUSTOM_MAIL_PROVIDER {
            (
                fl!("custom-mail-update-password"),
                fl!("custom-mail-update-password"),
                Message::OpenDialog(super::DialogPage::CustomMail(Some(account.id))),
            )
        } else {
            (
                fl!("reconnect-account"),
                fl!("sign-in-again"),
                Message::StartAuth(account.provider.clone(), Some(account.id)),
            )
        };
        widget::settings::section()
            .title(fl!("account"))
            .add(
                widget::settings::item::builder(fl!("account-enabled"))
                    .description(fl!("account-enabled-description"))
                    .toggler(account.enabled, Message::SetAccountEnabled),
            )
            .add(
                widget::settings::item::builder(label)
                    .control(widget::button::standard(action).on_press(message)),
            )
            .into()
    }

    fn services_section<'a>(&self, account: &'a Account) -> Element<'a, Message> {
        let mut section = widget::settings::section().header(
            widget::Column::new()
                .push(widget::text::heading(fl!("services")))
                .push(
                    widget::text::caption(fl!("services-description"))
                        .class(cosmic::style::Text::Default),
                )
                .spacing(spacing().space_xxxs),
        );

        for (service, enabled) in &account.services {
            let (name, description, icon) = service_details(service);
            let service = service.clone();

            section = section.add(
                widget::settings::item::builder(name)
                    .description(description)
                    .icon(widget::icon::from_name(icon).size(16))
                    .toggler(*enabled, move |enabled| {
                        Message::SetServiceEnabled(service.clone(), enabled)
                    }),
            );
        }

        section.into()
    }

    fn grants_section(&self) -> Element<'_, Message> {
        let mut section = widget::settings::section().header(
            widget::Column::new()
                .push(widget::text::heading(fl!("app-access")))
                .push(
                    widget::text::caption(fl!("app-access-description"))
                        .class(cosmic::style::Text::Default),
                )
                .spacing(spacing().space_xxxs),
        );

        if self.grants.is_empty() {
            return section
                .add(widget::text::body(fl!("no-app-access")).class(cosmic::style::Text::Default))
                .into();
        }

        for (service, caller_identity, decision) in &self.grants {
            let label = caller_label(caller_identity);
            let summary = if decision == "deny" {
                fl!("grant-blocked", service = service.as_str())
            } else {
                fl!("grant-allowed", service = service.as_str())
            };
            let (service, caller) = (service.clone(), caller_identity.clone());

            section = section.add(
                widget::settings::item::builder(label)
                    .description(summary)
                    .control(
                        widget::button::text(fl!("revoke-access"))
                            .on_press(Message::RevokeGrant(service, caller)),
                    ),
            );
        }

        section.into()
    }

    fn details_section<'a>(&self, account: &'a Account) -> Element<'a, Message> {
        let mut section = widget::settings::section()
            .title(fl!("details"))
            .add(widget::settings::flex_item(
                fl!("provider"),
                widget::text::body(self.provider_name(&account.provider)),
            ))
            .add(widget::settings::flex_item(
                fl!("created-at"),
                widget::text::body(format_timestamp(account.created_at)),
            ))
            .add(widget::settings::flex_item(
                fl!("last-used"),
                widget::text::body(
                    account
                        .last_used
                        .map(format_timestamp)
                        .unwrap_or_else(|| fl!("no-usage")),
                ),
            ));
        if let Some(settings) = &account.custom_mail {
            section = section
                .add(widget::settings::flex_item(
                    fl!("custom-mail-imap"),
                    widget::text::body(format!("{}:{}", settings.imap_host, settings.imap_port)),
                ))
                .add(widget::settings::flex_item(
                    fl!("custom-mail-smtp"),
                    widget::text::body(format!("{}:{}", settings.smtp_host, settings.smtp_port)),
                ));
        }
        if let Some(url) = &account.caldav_url {
            section = section.add(widget::settings::flex_item(
                fl!("caldav-server-url"),
                widget::text::body(url.as_str()),
            ));
        }
        section.into()
    }

    /// Shared by the welcome screen and add-account dialog.
    pub(super) fn provider_list(&self) -> Element<'_, Message> {
        let mut rows: Vec<Element<'_, Message>> = self
            .providers
            .iter()
            .map(|provider| {
                provider_row(
                    self.provider_icon(&provider.id, 32),
                    provider.name.clone(),
                    "go-next-symbolic",
                    Message::StartAuth(provider.id.clone(), None),
                )
            })
            .collect();

        rows.push(provider_row(
            widget::icon::from_name("list-add-symbolic").size(32).into(),
            fl!("add-other-account"),
            if self.other_accounts_expanded {
                "go-down-symbolic"
            } else {
                "go-next-symbolic"
            },
            Message::ToggleOtherAccounts,
        ));
        if self.other_accounts_expanded {
            rows.push(provider_row(
                self.provider_icon(CUSTOM_MAIL_PROVIDER, 32),
                fl!("custom-mail-title"),
                "go-next-symbolic",
                Message::OpenDialog(super::DialogPage::CustomMail(None)),
            ));
            rows.push(provider_row(
                self.provider_icon(CUSTOM_CALDAV_PROVIDER, 32),
                fl!("caldav-title"),
                "go-next-symbolic",
                Message::OpenDialog(super::DialogPage::CalDav(None)),
            ));
        }
        widget::Column::with_children(rows)
            .spacing(spacing().space_xs)
            .width(Length::Fill)
            .into()
    }

    pub(super) fn provider_name(&self, provider_id: &str) -> String {
        if provider_id == CUSTOM_CALDAV_PROVIDER {
            return fl!("caldav-provider-name");
        }
        if provider_id == CUSTOM_MAIL_PROVIDER {
            return fl!("custom-mail-provider-name");
        }
        self.providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .map_or_else(|| provider_id.to_string(), |provider| provider.name.clone())
    }

    pub(super) fn provider_icon(&self, provider_id: &str, size: u16) -> Element<'static, Message> {
        widget::icon(self.provider_icon_handle(provider_id))
            .size(size)
            .width(Length::Fixed(f32::from(size)))
            .height(Length::Fixed(f32::from(size)))
            .into()
    }

    pub(super) fn provider_icon_handle(&self, provider_id: &str) -> widget::icon::Handle {
        let source = self
            .providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .and_then(|provider| provider.icon_source());

        match source {
            Some(IconSource::Url(url)) => match self.icon_cache.get(&url) {
                Some(handle) => handle.clone(),
                None => bundled_provider_icon(provider_id),
            },
            Some(IconSource::Path(path)) => widget::icon::from_path(path),
            Some(IconSource::ThemeName(name)) => widget::icon::from_name(name.as_str()).into(),
            None => bundled_provider_icon(provider_id),
        }
    }

    fn empty_state<'a>(
        &self,
        icon: &'static str,
        title: String,
        body: String,
        action: Option<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        widget::Column::new()
            .push(widget::icon::from_name(icon).size(56))
            .push(widget::text::title3(title).align_x(Horizontal::Center))
            .push(
                widget::text::body(body)
                    .align_x(Horizontal::Center)
                    .width(Length::Fixed(PROSE_WIDTH))
                    .class(cosmic::style::Text::Default),
            )
            .push_maybe(action)
            .align_x(Alignment::Center)
            .spacing(spacing().space_s)
            .apply(widget::container)
            .center(Length::Fill)
            .padding(spacing().space_l)
            .into()
    }
}

fn warning_banner(message: String) -> Element<'static, Message> {
    widget::Row::new()
        .push(widget::icon::from_name("dialog-warning-symbolic").size(16))
        .push(widget::text::body(message).width(Length::Fill))
        .spacing(spacing().space_xs)
        .align_y(Alignment::Center)
        .apply(widget::container)
        .class(cosmic::style::Container::Card)
        .padding(spacing().space_s)
        .width(Length::Fill)
        .into()
}

fn provider_row(
    icon: Element<'static, Message>,
    label: String,
    chevron: &'static str,
    message: Message,
) -> Element<'static, Message> {
    widget::Row::new()
        .push(icon)
        .push(widget::text::body(label.clone()).width(Length::Fill))
        .push(widget::icon::from_name(chevron).size(16))
        .spacing(spacing().space_s)
        .align_y(Alignment::Center)
        .apply(widget::button::custom)
        .padding([spacing().space_s, spacing().space_m])
        .width(Length::Fill)
        .name(label)
        .on_press(message)
        .into()
}

fn bundled_provider_icon(provider_id: &str) -> widget::icon::Handle {
    match provider_id {
        "google" => widget::icon::from_raster_bytes(GOOGLE_ICON),
        "microsoft" => widget::icon::from_raster_bytes(MICROSOFT_ICON),
        CUSTOM_MAIL_PROVIDER => widget::icon::from_svg_bytes(MAIL_ICON),
        CUSTOM_CALDAV_PROVIDER => widget::icon::from_svg_bytes(CALENDAR_ICON),
        _ => widget::icon::from_name("network-server-symbolic").into(),
    }
}

fn service_details(service: &Service) -> (String, String, &'static str) {
    match service {
        Service::Email => (
            fl!("service-email"),
            fl!("service-email-description"),
            "mail-message-new-symbolic",
        ),
        Service::Calendar => (
            fl!("service-calendar"),
            fl!("service-calendar-description"),
            "x-office-calendar-symbolic",
        ),
        Service::Contacts => (
            fl!("service-contacts"),
            fl!("service-contacts-description"),
            "x-office-address-book-symbolic",
        ),
        Service::Tasks => (
            fl!("service-tasks"),
            fl!("service-tasks-description"),
            "checkbox-checked-symbolic",
        ),
    }
}

pub(super) fn caller_label(caller_identity: &str) -> String {
    if caller_identity.starts_with('/') {
        caller_identity
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or(caller_identity)
            .to_string()
    } else {
        caller_identity.to_string()
    }
}

fn format_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%B %d, %Y at %I:%M %p")
        .to_string()
}
