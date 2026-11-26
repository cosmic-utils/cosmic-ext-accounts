<div align="center">
    <img src="assets/icon.svg" width="250" alt="Accounts icon" />
    <h1>Accounts</h1>
    <p>A Rust library for managing online accounts in COSMIC™ apps</p>
</div>

Each application picks the service features it needs, stores its own accounts
in its own data directory, and owns its own credentials.

## Features

- Google and Microsoft OAuth authorization-code sign-in with PKCE and an
  ephemeral `127.0.0.1` redirect listener.
- Service-scoped identity and tokens: Calendar, Mail, Contacts, and Tasks do not
  share accounts.
- Separate Microsoft Graph and Exchange Online registrations so Graph tokens
  are never reused for IMAP/SMTP.
- Custom CalDAV, IMAP/SMTP, CardDAV, and LDAP account forms, gated by features.
- An app-private encrypted credential store. The master secret lives in macOS
  Keychain, Windows Credential Manager, or the XDG Secret portal on Linux.
  Locked or unavailable secure storage prevents the store from opening.
- Token caching by account, resource, and scope set; serialized refresh and
  rotation; and generation counters that invalidate stale credentials after
  reconnect or removal.
- An optional `libcosmic` context-drawer panel with provider picker, pending
  sign-in page, account cards, and narrow custom forms.

## Add it to an application

```toml
[dependencies]
accounts = { git = "https://github.com/cosmic-ext-orbit/cosmic-ext-accounts", features = ["calendar", "libcosmic"] }
```

Features are additive and optional:

| Feature | Enables |
| --- | --- |
| `calendar` | Calendar accounts and CalDAV credentials |
| `mail` | Mail accounts, IMAP/SMTP, Graph, and Exchange credentials |
| `contacts` | Contacts accounts, CardDAV, and LDAP credentials |
| `tasks` | Tasks accounts and task credentials |
| `libcosmic` | Reusable COSMIC widgets; implies `open` for browser launch |

Headless builds work with any subset of the service features and without
`libcosmic`.

```rust
use accounts::{Accounts, Config, Provider, Service};

let config = Config::new("dev.example.Calendar")
    .with_client_from_env(Provider::Google, Service::Calendar)
    .with_client_from_env(Provider::Microsoft, Service::Calendar);

let accounts = Accounts::new(config).await?;
let calendar_accounts = accounts.list(Service::Calendar).await;
```

To sign in, keep the pending request before opening the browser so the UI can
show the wait state even if the browser launch fails:

```rust
let request = accounts::ui::begin_sign_in(&accounts, Provider::Google, Service::Calendar).await?;
accounts::ui::open_sign_in_url(&request)?;
let account = accounts.complete_auth(request).await?;
```

Feature-gated accessors return credentials for the calling app only:
`calendar_credentials(id)`, `mail_access(id)`, `contacts_credentials(id)`, and
`tasks_credentials(id)`.

## OAuth client configuration

Client registrations are supplied by the embedding application. The library
does not ship client IDs or secrets. `with_client_from_env` reads:

| Service | Google | Microsoft |
| --- | --- | --- |
| Calendar | `ACCOUNTS_GOOGLE_CALENDAR_CLIENT_ID` | `ACCOUNTS_MICROSOFT_CALENDAR_CLIENT_ID` |
| Mail | `ACCOUNTS_GOOGLE_MAIL_CLIENT_ID` | `ACCOUNTS_MICROSOFT_MAIL_CLIENT_ID` |
| Contacts | `ACCOUNTS_GOOGLE_CONTACTS_CLIENT_ID` | `ACCOUNTS_MICROSOFT_CONTACTS_CLIENT_ID` |
| Tasks | `ACCOUNTS_GOOGLE_TASKS_CLIENT_ID` | `ACCOUNTS_MICROSOFT_TASKS_CLIENT_ID` |

Mail also accepts `ACCOUNTS_MICROSOFT_MAIL_EXCHANGE_CLIENT_ID` for the separate
Exchange Online IMAP/SMTP registration. Each prefix supports optional
`_CLIENT_SECRET` and `_REDIRECT_URI` variables. The redirect URI must use
`http://127.0.0.1`; the library replaces the port with an ephemeral listener
port for every sign-in. Registrations can also be injected directly with
`Config::with_client` and `Config::with_resource_client`.

## Reusable UI

With `libcosmic`, `accounts::ui::view` renders the shared Accounts page. Hosts
provide localized `Labels`, the current accounts and pending request, and
callbacks for add, cancel, remove, reconnect, toggle, and optional per-account
sync. The library filters providers by the runtime `Service`, so a combined
build only shows the providers and forms valid for that service. Hosts own the
state and deliver errors through their toaster.

## Development

```sh
just build
just test
just check-features
just check
```

`just check-features` verifies headless builds, each individual service
feature, and the full `libcosmic` build.

## Applications using this library

- [Calendar](https://github.com/cosmic-ext-orbit/cosmic-ext-calendar)
- [Mail](https://github.com/cosmic-ext-orbit/cosmic-ext-mail)
- [Tasks](https://github.com/cosmic-ext-orbit/cosmic-ext-tasks)

## Contributing

Contributions are welcome. Please open an issue before starting larger changes.

## Related projects

- [GNOME Online Accounts](https://gitlab.gnome.org/GNOME/gnome-online-accounts) - inspiration for this project
- [libcosmic](https://github.com/pop-os/libcosmic) - COSMIC UI toolkit
- [COSMIC Desktop](https://github.com/pop-os/cosmic-epoch) - the COSMIC desktop environment

## Support

- [GitHub Issues](https://github.com/cosmic-ext-orbit/cosmic-ext-accounts/issues)
- [COSMIC Discord](https://discord.gg/cosmic-desktop)
- [System76 Community](https://chat.pop-os.org/)
