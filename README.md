<div align="center">
  <br>
  <img src="crates/ui/resources/icons/hicolor/scalable/apps/icon.svg" width="128" alt="Accounts for COSMIC icon">
  <h1>Accounts for COSMIC™</h1>

  <p>Sign in once, use everywhere</p>

  ![window-light.png](crates/ui/resources/screenshots/welcome-light.png#gh-light-mode-only)
  ![window-dark.png](crates/ui/resources/screenshots/welcome-dark.png#gh-dark-mode-only)
</div>

Add your online account once, and every COSMIC app that needs it can use it. No more signing in
separately in each app, and no passwords stored by apps that don't need them.

## Installation

Install a configured release package with `sudo apt install ./cosmic-ext-accounts_VERSION_ARCH.deb`.
For a source checkout, `just install` builds the app and installs its shared files, including the
polkit policy. Source installs preserve existing system provider manifests. After an upgrade,
sign out and back in so the session loads the new daemon. Open **Accounts for COSMIC**
from your app launcher to sign in.

The daemon runs on each user's session bus. Access and refresh tokens are stored in that user's
Secret Service keyring, and grants remain in the user's profile. The broker checks the caller's Unix
UID, polkit authorization, and app consent before issuing tokens. Each daemon uses its own loopback
port for OAuth redirects.

System provider manifests are installed under `/usr/share/accounts/providers`. For a source
installation, copy the template you need to `${XDG_DATA_HOME:-$HOME/.local/share}/accounts/providers`
and set the client configuration there. User overrides take precedence over the system defaults.
Keep private OAuth credentials in a user-only file. For shared desktop client settings, export
`GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, and `MICROSOFT_CLIENT_ID`, then run
`just configure-providers /usr/share/accounts/providers/`. The recipe uses `sudo` to install the
configured manifests, which are readable by all local users. Sign out and back in after updating
either location so the daemon reloads the manifests.

### Building a release package

On the Debian-based release you want to support, install `dpkg-dev` and set `GOOGLE_CLIENT_ID`,
`GOOGLE_CLIENT_SECRET`, and `MICROSOFT_CLIENT_ID` for distributable native desktop clients.
Run `just package-native` to build the package under `target/native/`. The provider registrations
must accept an ephemeral port on the configured `http://127.0.0.1` loopback redirect. The package
includes the shared desktop client configuration.

## Contributing

Contributions are welcome! Please read our [Contributing Guidelines](CONTRIBUTING.md) and [Code of Conduct](CODE_OF_CONDUCT.md).

## Related Projects

- [GNOME Online Accounts](https://gitlab.gnome.org/GNOME/gnome-online-accounts) - Inspiration for this project
- [libcosmic](https://github.com/pop-os/libcosmic) - COSMIC UI toolkit
- [COSMIC Desktop](https://github.com/pop-os/cosmic-epoch) - The COSMIC desktop environment

## Support

- [GitHub Issues](https://github.com/cosmic-utils/accounts/issues)
- [COSMIC Discord](https://discord.gg/cosmic-desktop)
- [System76 Community](https://chat.pop-os.org/)
