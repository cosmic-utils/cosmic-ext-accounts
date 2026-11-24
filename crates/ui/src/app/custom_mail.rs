// SPDX-License-Identifier: GPL-3.0-only

use accounts_core::models::CustomMailSettings;

use crate::fl;

#[derive(Clone, Default)]
pub struct AccountPassword(pub String);

impl std::fmt::Debug for AccountPassword {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt.write_str("AccountPassword([redacted])")
    }
}

#[derive(Clone, Copy, Debug)]
pub enum MailField {
    Email,
    Username,
    ImapHost,
    ImapPort,
    SmtpHost,
    SmtpPort,
}

pub(super) struct MailForm {
    pub email: String,
    pub username: String,
    pub imap_host: String,
    pub imap_port: String,
    pub smtp_host: String,
    pub smtp_port: String,
    pub smtp_starttls: bool,
    pub password: AccountPassword,
}

impl Default for MailForm {
    fn default() -> Self {
        Self {
            email: String::new(),
            username: String::new(),
            imap_host: String::new(),
            imap_port: "993".into(),
            smtp_host: String::new(),
            smtp_port: "587".into(),
            smtp_starttls: true,
            password: AccountPassword::default(),
        }
    }
}

impl MailForm {
    pub fn change(&mut self, field: MailField, value: String) {
        *match field {
            MailField::Email => &mut self.email,
            MailField::Username => &mut self.username,
            MailField::ImapHost => &mut self.imap_host,
            MailField::ImapPort => &mut self.imap_port,
            MailField::SmtpHost => &mut self.smtp_host,
            MailField::SmtpPort => &mut self.smtp_port,
        } = value;
    }

    pub fn settings(&self) -> Result<CustomMailSettings, String> {
        let invalid_port = || fl!("custom-mail-invalid-port");
        let imap_port = self.imap_port.trim().parse().map_err(|_| invalid_port())?;
        let smtp_port = self.smtp_port.trim().parse().map_err(|_| invalid_port())?;
        if imap_port == 0 || smtp_port == 0 {
            return Err(invalid_port());
        }
        Ok(CustomMailSettings {
            imap_host: self.imap_host.clone(),
            imap_port,
            smtp_host: self.smtp_host.clone(),
            smtp_port,
            smtp_starttls: self.smtp_starttls,
        })
    }
}
