use std::fmt;

/// Stable, consumer-facing classification of errors reported by the Accounts broker.
///
/// The daemon defines the concrete D-Bus error names in its `TokenError`, but
/// those names are part of the public protocol. Keeping the mapping here lets
/// every consumer react to the same failures without reimplementing string
/// matching against `zbus` errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountsError {
    /// The broker could not be reached, or an unrecognised failure occurred.
    Unavailable,
    /// The daemon is too old to provide the requested endpoint or property.
    UpgradeRequired,
    /// The account or service is disabled, or the returned data is unusable.
    Disabled,
    /// Credentials are missing or could not be refreshed; reauthenticate.
    NeedsReauth,
    /// The user or policy refused authorization.
    AccessDenied,
    /// A consent prompt was shown but not answered.
    ConsentTimeout,
    /// The broker reported a generic operation failure.
    Failed,
    /// The endpoint advertises a setting this client cannot use.
    Unsupported,
}

impl AccountsError {
    /// Classifies an error received from a D-Bus call into a stable kind.
    pub fn from_zbus(error: &zbus::Error) -> Self {
        match error {
            zbus::Error::MethodError(name, _, _) => match name.as_str() {
                "org.freedesktop.DBus.Error.UnknownProperty"
                | "org.freedesktop.DBus.Error.UnknownMethod" => Self::UpgradeRequired,
                "dev.edfloreshz.Accounts.Error.Failed" => Self::Failed,
                "dev.edfloreshz.Accounts.Error.NeedsReauth" => Self::NeedsReauth,
                "dev.edfloreshz.Accounts.Error.AccessDenied"
                | "org.freedesktop.DBus.Error.AccessDenied" => Self::AccessDenied,
                "dev.edfloreshz.Accounts.Error.ConsentTimeout" => Self::ConsentTimeout,
                "dev.edfloreshz.Accounts.Error.Disabled"
                | "org.freedesktop.DBus.Error.UnknownObject"
                | "org.freedesktop.DBus.Error.UnknownInterface" => Self::Disabled,
                _ => Self::Unavailable,
            },
            zbus::Error::FDO(error) => Self::from_fdo(error),
            zbus::Error::InterfaceNotFound => Self::UpgradeRequired,
            _ => Self::Unavailable,
        }
    }

    fn from_fdo(error: &zbus::fdo::Error) -> Self {
        match error {
            zbus::fdo::Error::UnknownProperty(_) | zbus::fdo::Error::UnknownMethod(_) => {
                Self::UpgradeRequired
            }
            zbus::fdo::Error::UnknownObject(_)
            | zbus::fdo::Error::UnknownInterface(_)
            | zbus::fdo::Error::NotSupported(_) => Self::Disabled,
            zbus::fdo::Error::AccessDenied(_) | zbus::fdo::Error::AuthFailed(_) => {
                Self::AccessDenied
            }
            _ => Self::Failed,
        }
    }

    /// Stable key suitable for app-local localization lookups.
    pub fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::UpgradeRequired => "upgrade-required",
            Self::Disabled => "disabled",
            Self::NeedsReauth => "needs-reauth",
            Self::AccessDenied => "access-denied",
            Self::ConsentTimeout => "consent-timeout",
            Self::Failed => "failed",
            Self::Unsupported => "unsupported",
        }
    }
}

impl From<zbus::Error> for AccountsError {
    fn from(error: zbus::Error) -> Self {
        Self::from_zbus(&error)
    }
}

impl From<zbus::fdo::Error> for AccountsError {
    fn from(error: zbus::fdo::Error) -> Self {
        Self::from_fdo(&error)
    }
}

impl fmt::Display for AccountsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for AccountsError {}

pub type Result<T> = std::result::Result<T, AccountsError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_fdo_errors_to_stable_kinds() {
        assert_eq!(
            AccountsError::from(zbus::fdo::Error::UnknownProperty(
                "mail_endpoint_version".into()
            )),
            AccountsError::UpgradeRequired
        );
        assert_eq!(
            AccountsError::from(zbus::fdo::Error::AccessDenied("denied".into())),
            AccountsError::AccessDenied
        );
        assert_eq!(
            AccountsError::from(zbus::fdo::Error::Failed("boom".into())),
            AccountsError::Failed
        );
    }

    #[test]
    fn codes_are_stable_for_app_localization() {
        assert_eq!(AccountsError::NeedsReauth.code(), "needs-reauth");
        assert_eq!(AccountsError::UpgradeRequired.code(), "upgrade-required");
    }
}
