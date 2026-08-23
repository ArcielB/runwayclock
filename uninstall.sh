#!/usr/bin/env bash
set -euo pipefail

purge_data="no"
keep_widget="no"

usage() {
    printf '%s\n' \
        "Uninstall RunwayClock for the current Linux user." \
        "" \
        "Usage: runwayclock-uninstall [--purge-data] [--keep-widget]" \
        "" \
        "By default, imported statements, corrections, facts, and snapshots are" \
        "kept so a later reinstall restores the same RunwayClock state."
}

while (( $# > 0 )); do
    case "$1" in
        --purge-data)
            purge_data="yes"
            shift
            ;;
        --keep-widget)
            keep_widget="yes"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
bin_home="${RUNWAYCLOCK_BIN_DIR:-$HOME/.local/bin}"
data_root="$data_home/runwayclock"
install_root="${RUNWAYCLOCK_INSTALL_ROOT:-$data_root/product}"
application_home="$data_home/applications"
widget_uuid="runwayclock@runwayclock.local"
widget_dir="$data_home/gnome-shell/extensions/$widget_uuid"

case "$install_root" in
    ""|/|"$HOME"|"$data_home"|"$data_root")
        echo "Refusing unsafe installation directory: $install_root" >&2
        exit 1
        ;;
esac

remove_our_link() {
    local path="$1"
    if [[ -L "$path" && "$(readlink -- "$path")" == "$install_root/"* ]]; then
        rm -- "$path"
    fi
}

remove_our_link "$bin_home/runwayclock-app"
remove_our_link "$bin_home/runwayclock-update"
remove_our_link "$bin_home/runwayclock-uninstall"
rm -f -- "$application_home/app.runwayclock.desktop.desktop"
if [[ -d "$install_root" ]]; then
    rm -rf -- "$install_root"
fi

if [[ "$keep_widget" != "yes" ]]; then
    if command -v gnome-extensions >/dev/null; then
        gnome-extensions disable "$widget_uuid" >/dev/null 2>&1 || true
        gnome-extensions uninstall "$widget_uuid" >/dev/null 2>&1 || true
    fi
    if [[ -d "$widget_dir" ]]; then
        rm -rf -- "$widget_dir"
    fi
    if command -v gsettings >/dev/null; then
        enabled_extensions="$(gsettings get org.gnome.shell enabled-extensions 2>/dev/null || true)"
        if [[ -n "$enabled_extensions" ]]; then
            enabled_extensions="$(printf '%s\n' "$enabled_extensions" \
                | sed -e "s/'$widget_uuid', //g" \
                      -e "s/, '$widget_uuid'//g" \
                      -e "s/'$widget_uuid'//g")"
            gsettings set org.gnome.shell enabled-extensions "$enabled_extensions" \
                >/dev/null 2>&1 || true
        fi
    fi
fi

if command -v update-desktop-database >/dev/null; then
    update-desktop-database "$application_home" >/dev/null 2>&1 || true
fi

if [[ "$purge_data" == "yes" ]]; then
    case "$data_root" in
        ""|/|"$HOME"|"$data_home")
            echo "Refusing unsafe data directory: $data_root" >&2
            exit 1
            ;;
    esac
    [[ ! -d "$data_root" ]] || rm -rf -- "$data_root"
    echo "RunwayClock and its local financial data were removed."
else
    echo "RunwayClock was removed."
    echo "Your local financial data remains at $data_root"
    echo "Run again with --purge-data only if you intentionally want to delete it."
fi
