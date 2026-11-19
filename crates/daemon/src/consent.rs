//! Calls the D-Bus-activated consent prompt when a token request has no stored grant.

use std::time::Duration;

pub const CONSENT_TIMEOUT: Duration = Duration::from_secs(120);

pub enum ConsentError {
    Timeout,
    Failed(String),
}

#[zbus::proxy(
    interface = "dev.edfloreshz.Accounts.ConsentPrompt",
    default_service = "dev.edfloreshz.Accounts.ConsentPrompt",
    default_path = "/dev/edfloreshz/Accounts/ConsentPrompt",
    gen_blocking = false
)]
trait ConsentPrompt {
    /// Shows the prompt and blocks until the user chooses. `true` = allow.
    fn prompt(
        &self,
        caller_name: &str,
        account_name: &str,
        provider_id: &str,
        service: &str,
    ) -> zbus::Result<bool>;
}

pub async fn prompt(
    connection: &zbus::Connection,
    caller_name: &str,
    account_name: &str,
    provider_id: &str,
    service: &str,
) -> std::result::Result<bool, ConsentError> {
    let proxy = ConsentPromptProxy::new(connection)
        .await
        .map_err(|e| ConsentError::Failed(e.to_string()))?;

    match tokio::time::timeout(
        CONSENT_TIMEOUT,
        proxy.prompt(caller_name, account_name, provider_id, service),
    )
    .await
    {
        Ok(Ok(decision)) => Ok(decision),
        Ok(Err(e)) => Err(ConsentError::Failed(e.to_string())),
        Err(_) => Err(ConsentError::Timeout),
    }
}
