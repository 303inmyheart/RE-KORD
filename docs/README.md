# RE-KORD documentation

## For listeners and hub owners

| Document | What it covers |
|---|---|
| [User guide](user-guide.md) | Every view and feature: library, player, Studio, Plectr, statistics, settings, remote access, backup, admin panel |
| [Installation](install.md) | Linux (AppImage, `.deb`, headless with systemd), Windows, Android, Docker, firewall, data locations |
| [Upgrading from legacy RE-KORD](upgrading-from-legacy.md) | Moving from the Electron / Node app: port change, data import, Android reinstall, Docker volumes |
| [Supported formats](supported-formats.md) | Audio formats, conversions, tags, covers, library layouts |
| [Deployment reference](DEPLOY.md) | Every flag and environment variable, systemd, Docker, the desktop server flavor, reverse proxies |
| [Security policy](../SECURITY.md) | Reporting vulnerabilities and the hub's security model |

## For developers and contributors

| Document | What it covers |
|---|---|
| [Contributing](../CONTRIBUTING.md) | Workflow, code style, checks, i18n rules, commits and pull requests |
| [Development](development.md) | Prerequisites, commands, tests, packaging, Android toolchain, release checklist |
| [Architecture](architecture.md) | How the hub, client and shells fit together; data on disk |
| [HTTP API](API.md) | `/api/v1` endpoints, permissions, CORS, transcode, downloads, backup format, error codes |
| [Android](ANDROID.md) | The Android shell: pairing, background playback, Cast, signing |
| [Translations](TRANSLATIONS.md) | Where strings live, adding a language, German style guide |
| [Module manifest](MODULES.md) | The reserved optional-module manifest |
| [Changelog](../CHANGELOG.md) | Release history |

## Images

Screenshots used in the README and the user guide are in
[`images/screenshots/`](images/screenshots/).
