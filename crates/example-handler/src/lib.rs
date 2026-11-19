// SPDX-License-Identifier: GPL-3.0-only

//! Reference provider handler that simulates sign-in and credential rotation.

use std::collections::HashMap;

use accounts_core::proxy::HANDLER_BLOB_PARAM;
use base64::Engine;

pub const BUS_NAME: &str = "dev.edfloreshz.Accounts.ProviderHandler.Example";
pub const OBJECT_PATH: &str = "/dev/edfloreshz/Accounts/ProviderHandler";

pub const INITIAL_BLOB: &[u8] = b"example-handler-credential-v1";
pub const REFRESH_SUFFIX: &[u8] = b"+refreshed";

pub struct ExampleHandler;

#[zbus::interface(name = "dev.edfloreshz.Accounts.ProviderHandler")]
impl ExampleHandler {
    /// On refresh, `params` includes the previous blob as base64 under `HANDLER_BLOB_PARAM`.
    async fn authenticate(
        &self,
        params: HashMap<String, String>,
    ) -> zbus::fdo::Result<(String, Vec<u8>)> {
        let identity = params
            .get("login_hint")
            .cloned()
            .unwrap_or_else(|| "device-user@example.com".to_string());

        if let Some(encoded) = params.get(HANDLER_BLOB_PARAM) {
            let mut blob = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|e| {
                    zbus::fdo::Error::InvalidArgs(format!("bad {HANDLER_BLOB_PARAM}: {e}"))
                })?;
            blob.extend_from_slice(REFRESH_SUFFIX);
            tracing::info!(%identity, "refreshed credential for handler account");
            return Ok((identity, blob));
        }

        tracing::info!(%identity, "issued credential for new handler account");
        Ok((identity, INITIAL_BLOB.to_vec()))
    }
}

/// Serves the example handler on the session bus.
pub async fn run() -> zbus::Result<()> {
    let _connection = zbus::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, ExampleHandler)?
        .build()
        .await?;
    tracing::info!("{BUS_NAME} ready at {OBJECT_PATH}");
    std::future::pending::<()>().await;
    Ok(())
}
