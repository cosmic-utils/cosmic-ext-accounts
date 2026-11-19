// SPDX-License-Identifier: GPL-3.0-only

//! Standalone consent prompt.

use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> cosmic::iced::Result {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("accounts_consent=info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    accounts_consent::run()
}
