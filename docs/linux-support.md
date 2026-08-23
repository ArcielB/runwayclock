# Linux support and distribution

Pop!_OS is RunwayClock's primary hands-on Linux test environment. Release
architecture is deliberately broader than that one distribution.

## Supported release paths

| Path | Intended systems | Root required | Desktop integration |
|---|---|---:|---|
| RunwayClock installer | Most x86_64 and ARM64 desktop Linux | No | Application menu, CLI launchers, optional GNOME widget |
| AppImage | Most contemporary desktop Linux | No | Manual unless the installer is used |
| Debian package | Debian, Ubuntu, Pop!_OS, and derivatives | Yes | Native application menu and package manager |
| RPM package | Fedora-family and compatible RPM systems | Yes | Native application menu and package manager |
| Source build | Other distributions and contributors | Build dependencies only | Development workflow |

The universal installer extracts the release AppImage into the user's XDG data
directory. This avoids a FUSE dependency and makes upgrades atomic. Application
launchers live below `~/.local`, while the database remains separately stored at:

```text
${XDG_DATA_HOME:-~/.local/share}/runwayclock/runwayclock.db
```

Updating or uninstalling the application does not delete that database. Data is
removed only with the explicit `runwayclock-uninstall --purge-data` option.

## Compatibility baseline

Official Linux artifacts are built on Ubuntu 22.04, one of Tauri 2's suitable
WebKitGTK 4.1 build baselines. This intentionally avoids linking releases on the
newest available distribution. Systems older than that baseline may have an
incompatible glibc and are not currently guaranteed.

The financial engine, SQLite layer, CSV importer, and CLI do not depend on a
desktop environment. The Tauri desktop uses the system's GTK/WebKit integration
on both X11 and Wayland. Packaging-specific failures must remain outside the
financial core.

## GNOME widget

The widget is optional and database-blind. The installer detects GNOME Shell and
selects the appropriate implementation:

- GNOME 40–44: legacy GJS extension API;
- GNOME 45–50: ES-module extension API.

Other desktop environments can run the full desktop application and CLI without
the widget. Future KDE, Cinnamon, or other integrations should consume the same
sanitized `widget.json` snapshot rather than opening SQLite.

## Architectures

GitHub releases build natively on x86_64 and ARM64 Ubuntu 22.04 runners. The
installer gives an actionable error on architectures without a matching release
instead of downloading an incompatible executable.

## Release integrity

Every release includes `SHA256SUMS`. The installer refuses missing or mismatched
entries before activating an update. Linux packages are not yet cryptographically
signed with a long-lived RunwayClock release key; checksums protect against
corruption but are not a substitute for future package signing.
