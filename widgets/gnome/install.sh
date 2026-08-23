#!/usr/bin/env bash
set -euo pipefail

widget_uuid="runwayclock@runwayclock.local"
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
extension_root="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions"
install_dir="$extension_root/$widget_uuid"

mkdir -p "$install_dir"
cp "$script_dir/$widget_uuid/metadata.json" "$install_dir/metadata.json"
cp "$script_dir/$widget_uuid/extension.js" "$install_dir/extension.js"

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
    echo "RunwayClock enabled."
else
    echo "RunwayClock installed and marked enabled."
    echo "Log out and back in once so GNOME Shell discovers it."
fi
