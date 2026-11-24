use std::io::ErrorKind;
use std::process::Command;

use crate::error::{AccountsError, Result};

/// Opens the Accounts app on its add-account page.
///
/// Consumers should not need to know the Accounts binary name or its CLI
/// flags. Both the installed name (`accounts_ui`) and the hyphenated spelling
/// used by some packaging setups are tried, so callers cannot get this wrong.
pub fn open_add_account() -> Result<()> {
    let mut last_error = None;

    for binary in ["accounts_ui", "accounts-ui"] {
        match Command::new(binary).arg("--add-account").spawn() {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                last_error = Some(error);
            }
            Err(error) => return Err(map_io(error)),
        }
    }

    Err(last_error.map(map_io).unwrap_or(AccountsError::Failed))
}

fn map_io(error: std::io::Error) -> AccountsError {
    match error.kind() {
        ErrorKind::PermissionDenied => AccountsError::AccessDenied,
        _ => AccountsError::Failed,
    }
}
