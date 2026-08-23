#!/usr/bin/env bash
set -euo pipefail

widget_uuid="runwayclock@runwayclock.local"
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
extension_root="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions"
install_dir="$extension_root/$widget_uuid"

if ! command -v gnome-shell >/dev/null; then
    echo "GNOME Shell was not detected; the optional panel widget was not installed."
    exit 0
fi

shell_version="$(gnome-shell --version 2>/dev/null || true)"
shell_major="$(printf '%s\n' "$shell_version" | sed -n 's/.* \([0-9][0-9]*\)\..*/\1/p')"
if [[ -z "$shell_major" ]]; then
    echo "Could not determine the GNOME Shell version from: $shell_version" >&2
    exit 1
fi

if (( shell_major >= 45 && shell_major <= 50 )); then
    extension_source="$script_dir/$widget_uuid/extension-modern.js"
    metadata_source="$script_dir/$widget_uuid/metadata-modern.json"
elif (( shell_major >= 40 )); then
    extension_source="$script_dir/$widget_uuid/extension.js"
    metadata_source="$script_dir/$widget_uuid/metadata.json"
else
    echo "GNOME Shell $shell_major is not supported by the RunwayClock widget." >&2
    echo "The RunwayClock desktop application can still be used normally." >&2
    exit 0
fi

if command -v gnome-extensions >/dev/null \
    && gnome-extensions list | grep -Fxq "$widget_uuid"; then
    # Disable first so an update reloads code instead of retaining the old class.
    gnome-extensions disable "$widget_uuid" 2>/dev/null || true
fi

mkdir -p "$install_dir"
cp "$metadata_source" "$install_dir/metadata.json"
cp "$extension_source" "$install_dir/extension.js"

if ! command -v gnome-extensions >/dev/null || ! command -v gsettings >/dev/null; then
    echo "RunwayClock copied to $install_dir."
    echo "GNOME Shell tools were not found, so enable it from your extension manager."
    exit 0
fi

# Preserve every existing preference while marking RunwayClock for activation.
# A newly copied extension is only discovered after GNOME Shell reloads.
enabled_extensions="$(gsettings get org.gnome.shell enabled-extensions)"
if [[ "$enabled_extensions" != *"'$widget_uuid'"* ]]; then
    if [[ "$enabled_extensions" == "@as []" || "$enabled_extensions" == "[]" ]]; then
        enabled_extensions="['$widget_uuid']"
    else
        enabled_extensions="${enabled_extensions%]}"
        enabled_extensions="$enabled_extensions, '$widget_uuid']"
    fi
    gsettings set org.gnome.shell enabled-extensions "$enabled_extensions"
fi
gsettings set org.gnome.shell disable-user-extensions false

if gnome-extensions list | grep -Fxq "$widget_uuid"; then
    gnome-extensions enable "$widget_uuid"
    echo "RunwayClock widget enabled for GNOME Shell $shell_major."
else
    echo "RunwayClock widget installed and marked enabled for GNOME Shell $shell_major."
    echo "Log out and back in once so GNOME Shell discovers it."
fi
