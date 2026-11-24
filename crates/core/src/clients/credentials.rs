use uuid::Uuid;
use zbus::Connection;

use crate::{
    error::{AccountsError, Result},
    models::Service,
    proxy::CredentialsProxy,
};

use super::account::account_path;

/// Opens the credentials interface for an account.
pub(super) async fn proxy(connection: &Connection, id: &Uuid) -> Result<CredentialsProxy<'static>> {
    CredentialsProxy::builder(connection)
        .path(account_path(id))
        .map_err(AccountsError::from)?
        .build()
        .await
        .map_err(AccountsError::from)
}

/// Requests an OAuth access token, optionally forcing a refresh first.
pub(super) async fn access_token(
    connection: &Connection,
    id: &Uuid,
    service: &Service,
    refresh: bool,
) -> Result<(String, i64)> {
    let credentials = proxy(connection, id).await?;
    if refresh {
        credentials
            .invalidate_token()
            .await
            .map_err(AccountsError::from)?;
    }
    let (token, expires_at) = credentials
        .get_access_token(&service.to_string())
        .await
        .map_err(AccountsError::from)?;
    validate_value(&token, AccountsError::NeedsReauth)?;
    Ok((token, expires_at))
}

/// Validates a DAV or directory URI before handing it to a transport.
pub(super) fn validate_url(uri: &str, scheme: &str) -> Result<()> {
    if uri.is_empty()
        || uri.chars().any(|c| c.is_control() || c.is_whitespace())
        || !uri.starts_with(scheme)
    {
        return Err(AccountsError::Disabled);
    }
    Ok(())
}

/// Validates a login name returned by the broker.
pub(super) fn validate_value(value: &str, invalid: AccountsError) -> Result<()> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(invalid);
    }
    Ok(())
}

/// Validates a password returned by the broker.
///
/// Unlike tokens, passwords may legitimately contain spaces, so only the
/// characters that cannot be represented in an IMAP/SMTP or DAV header are
/// rejected here.
pub(super) fn validate_password(password: &str, invalid: AccountsError) -> Result<()> {
    if password.is_empty() || password.chars().any(|c| matches!(c, '\r' | '\n' | '\0')) {
        return Err(invalid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_must_use_the_expected_secure_scheme() {
        assert!(validate_url("https://dav.example.com", "https://").is_ok());
        assert!(validate_url("ldaps://ldap.example.com", "ldaps://").is_ok());
        assert_eq!(
            validate_url("http://dav.example.com", "https://"),
            Err(AccountsError::Disabled)
        );
        assert_eq!(
            validate_url("https://dav.example.com/path with space", "https://"),
            Err(AccountsError::Disabled)
        );
    }

    #[test]
    fn secrets_and_identities_are_checked() {
        assert!(validate_value("user@example.com", AccountsError::Disabled).is_ok());
        assert_eq!(
            validate_value("user\n", AccountsError::Disabled),
            Err(AccountsError::Disabled)
        );
        assert!(validate_password("a pass phrase", AccountsError::Disabled).is_ok());
        assert_eq!(
            validate_password("line\nbreak", AccountsError::NeedsReauth),
            Err(AccountsError::NeedsReauth)
        );
    }
}
