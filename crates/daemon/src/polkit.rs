//! Checks system polkit policy before the daemon applies per-app grants.

use std::collections::HashMap;

use tokio::sync::OnceCell;
use zbus::message::Header;
use zbus_polkit::policykit1::{AuthorityProxy, CheckAuthorizationFlags, Subject};

/// `auth_self` for account mutations.
pub const ACTION_MANAGE_OWN_ACCOUNTS: &str = "dev.edfloreshz.Accounts.manage-own-accounts";
/// `allow_active: yes` for token requests.
pub const ACTION_GET_TOKEN: &str = "dev.edfloreshz.Accounts.get-token";
/// `auth_admin` — reserved for the not-yet-implemented `RegisterProvider`.
#[allow(dead_code)]
pub const ACTION_REGISTER_PROVIDER: &str = "dev.edfloreshz.Accounts.register-provider";

static SYSTEM_BUS: OnceCell<zbus::Connection> = OnceCell::const_new();
static SESSION_BUS: OnceCell<zbus::Connection> = OnceCell::const_new();

async fn system_bus() -> zbus::Result<&'static zbus::Connection> {
    SYSTEM_BUS.get_or_try_init(zbus::Connection::system).await
}

async fn session_bus() -> zbus::Result<&'static zbus::Connection> {
    SESSION_BUS.get_or_try_init(zbus::Connection::session).await
}

/// Denies access when polkit is unavailable or its check fails.
pub async fn check(header: &Header<'_>, action_id: &str) -> bool {
    match check_inner(header, action_id).await {
        Ok(authorized) => authorized,
        Err(err) => {
            tracing::warn!("polkit check for {action_id} failed, denying: {err}");
            false
        }
    }
}

async fn check_inner(header: &Header<'_>, action_id: &str) -> zbus::Result<bool> {
    // The system-bus authority needs a process subject for session-bus callers.
    let sender = header
        .sender()
        .ok_or_else(|| zbus::Error::Failure("message has no sender".to_string()))?;
    let dbus = zbus::fdo::DBusProxy::new(session_bus().await?).await?;
    let credentials = dbus
        .get_connection_credentials(zbus::names::BusName::from(sender.to_owned()))
        .await?;
    let pid = credentials
        .process_id()
        .ok_or_else(|| zbus::Error::Failure("caller has no pid".to_string()))?;

    let subject = Subject::new_for_owner(pid, None, None)
        .map_err(|e| zbus::Error::Failure(format!("could not build polkit subject: {e}")))?;
    let authority = AuthorityProxy::new(system_bus().await?).await?;
    let result = authority
        .check_authorization(
            &subject,
            action_id,
            &HashMap::new(),
            CheckAuthorizationFlags::AllowUserInteraction.into(),
            "",
        )
        .await?;
    Ok(result.is_authorized)
}
