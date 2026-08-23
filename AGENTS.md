# RunwayClock repository instructions

## Linux compatibility

- Pop!_OS is the primary hands-on development and testing environment. The
  current machine is useful evidence, but it is not the definition of Linux
  support.
- Design the application to work across as many practical Linux distributions,
  desktop environments, display servers, and maintained versions as possible.
- Keep `runway-core`, database, import, and forecasting code headless and free
  of distribution- or desktop-specific assumptions.
- Prefer standard Linux interfaces and XDG paths. Isolate unavoidable GNOME,
  GTK/WebKit, packaging, or distribution differences behind small adapters.
- The GNOME panel extension is one presentation integration. The desktop app
  and runway engine must remain usable without GNOME or without any widget.
- Do not hard-code Pop!_OS behavior into shared code. Feature-detect platform
  capabilities when possible, fail with an actionable explanation, and
  document any minimum Linux runtime requirements.
- Consider compatibility impact before raising GTK, WebKit, glibc, GNOME Shell,
  or packaging requirements. Add CI coverage or a documented test matrix when
  broadening Linux release work.
- AppImage and Debian packages are initial distribution formats, not permanent
  architectural boundaries. Leave room for RPM, Flatpak, and other community
  packaging without changing the financial core.

