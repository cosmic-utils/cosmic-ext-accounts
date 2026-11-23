// SPDX-License-Identifier: GPL-3.0-only

use crate::app::{
    AppModel, Message,
    custom_mail::{MailField, MailPassword},
};
use crate::fl;
use accounts_core::Uuid;
use cosmic::iced::{Alignment, Length};
use cosmic::prelude::*;
use cosmic::theme::spacing;
use cosmic::widget;

#[derive(Clone, Debug)]
pub enum DialogPage {
    /// Lets the user pick which provider to sign in with.
    AddAccount,
    CustomMail(Option<Uuid>),
    /// Waits for a sign-in that is happening in the browser, for the given provider.
    SigningIn(String),
    /// Confirms removal of the account with the given id and label.
    RemoveAccount(Uuid, String),
}

impl DialogPage {
    pub(super) fn view<'a>(&'a self, app: &'a AppModel) -> Element<'a, Message> {
        match self {
            DialogPage::AddAccount => widget::dialog()
                .title(fl!("add-account-title"))
                .body(fl!("add-account-body"))
                .control(app.provider_list())
                .primary_action(
                    widget::button::standard(fl!("cancel")).on_press(Message::CloseDialog),
                )
                .into(),

            DialogPage::CustomMail(existing) => app.custom_mail_dialog(*existing),

            DialogPage::SigningIn(provider) => {
                let provider_name = app.provider_name(provider);

                widget::dialog()
                    .title(fl!("signing-in-title"))
                    .body(fl!("signing-in-body", provider = provider_name.as_str()))
                    .icon(app.provider_icon(provider, 48))
                    .control(
                        widget::progress_bar::indeterminate_linear()
                            .width(Length::Fill)
                            .apply(widget::container)
                            .padding([spacing().space_s, 0]),
                    )
                    .primary_action(
                        widget::button::standard(fl!("cancel")).on_press(Message::CancelAuth),
                    )
                    .secondary_action(
                        widget::button::text(fl!("open-browser-again"))
                            .on_press(Message::OpenAuthUrl),
                    )
                    .into()
            }

            DialogPage::RemoveAccount(id, label) => widget::dialog()
                .title(fl!("remove-account-title", account = label.as_str()))
                .body(fl!("remove-account-body"))
                .icon(
                    widget::icon::from_name("dialog-warning-symbolic")
                        .size(32)
                        .apply(widget::container)
                        .align_y(Alignment::Center),
                )
                .primary_action(
                    widget::button::destructive(fl!("remove"))
                        .on_press(Message::RemoveAccount(*id)),
                )
                .secondary_action(
                    widget::button::standard(fl!("cancel")).on_press(Message::CloseDialog),
                )
                .into(),
        }
    }
}

impl AppModel {
    fn custom_mail_dialog<'a>(&'a self, existing: Option<Uuid>) -> Element<'a, Message> {
        let form = &self.mail_form;
        let editable = !self.saving_mail;
        let field = |label, value: &'a str, field| {
            widget::text_input(label, value)
                .editing(editable)
                .on_input(move |value| Message::ChangeMailField(field, value))
        };
        let server_fields = |host: &'a str, host_field, port: &'a str, port_field| {
            widget::settings::item_row(vec![
                field(fl!("custom-mail-host"), host, host_field)
                    .width(Length::Fill)
                    .into(),
                field(fl!("custom-mail-port"), port, port_field)
                    .width(Length::Fixed(88.0))
                    .into(),
            ])
        };
        let password =
            widget::secure_input(fl!("custom-mail-password"), &form.password.0, None, true)
                .editing(editable)
                .on_input(|value| Message::ChangeMailPassword(MailPassword(value)));
        let mut fields = widget::column::with_capacity(4).spacing(spacing().space_s);
        if existing.is_none() {
            fields = fields
                .push(
                    widget::settings::section()
                        .title(fl!("account"))
                        .add(field(
                            fl!("custom-mail-email"),
                            &form.email,
                            MailField::Email,
                        ))
                        .add(field(
                            fl!("custom-mail-username"),
                            &form.username,
                            MailField::Username,
                        ))
                        .add(password),
                )
                .push(
                    widget::settings::section()
                        .title(fl!("custom-mail-imap"))
                        .add(server_fields(
                            &form.imap_host,
                            MailField::ImapHost,
                            &form.imap_port,
                            MailField::ImapPort,
                        )),
                )
                .push(
                    widget::settings::section()
                        .title(fl!("custom-mail-smtp"))
                        .add(server_fields(
                            &form.smtp_host,
                            MailField::SmtpHost,
                            &form.smtp_port,
                            MailField::SmtpPort,
                        ))
                        .add(
                            widget::settings::item::builder(fl!("custom-mail-starttls")).control(
                                widget::toggler(form.smtp_starttls).on_toggle_maybe(
                                    editable.then_some(Message::ChangeMailStarttls),
                                ),
                            ),
                        ),
                );
        } else {
            fields = fields.push(password);
        }
        if let Some(error) = &self.mail_error {
            fields = fields.push(widget::text::body(error.as_str()));
        }
        widget::dialog()
            .title(if existing.is_some() {
                fl!("custom-mail-update-password")
            } else {
                fl!("custom-mail-title")
            })
            .max_width(520)
            .control(widget::scrollable(fields).height(Length::Shrink))
            .primary_action(
                widget::button::suggested(fl!("custom-mail-save"))
                    .on_press_maybe(editable.then_some(Message::SaveCustomMail(existing))),
            )
            .secondary_action(
                widget::button::standard(fl!("cancel"))
                    .on_press_maybe(editable.then_some(Message::CloseDialog)),
            )
            .into()
    }
}
