// SPDX-License-Identifier: GPL-3.0-only

use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

mod app;
mod i18n;

fn main() -> cosmic::iced::Result {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("accounts_ui=info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mode = std::env::args_os().nth(1);
    if mode.as_deref() == Some(std::ffi::OsStr::new("--daemon"))
        || std::env::var_os("ACCOUNTS_HEADLESS").is_some()
    {
        let runtime = tokio::runtime::Runtime::new().expect("failed to start tokio runtime");
        runtime
            .block_on(accounts_daemon::run())
            .expect("daemon exited with an error");
        return Ok(());
    }

    if mode.as_deref() == Some(std::ffi::OsStr::new("--consent-prompt"))
        || std::env::var_os("ACCOUNTS_CONSENT_PROMPT").is_some()
    {
        return accounts_consent::run();
    }

    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();

    i18n::init(&requested_languages);

    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(920.0, 640.0))
        .size_limits(
            cosmic::iced::Limits::NONE
                .min_width(400.0)
                .min_height(360.0),
        );

    let add_account = matches!(
        mode.as_deref().and_then(std::ffi::OsStr::to_str),
        Some("--add-account" | "cosmic-accounts://add-account")
    );
    cosmic::app::run::<app::AppModel>(settings, add_account)
}
