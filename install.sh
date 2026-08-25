#!/usr/bin/env bash
set -euo pipefail

repository="${RUNWAYCLOCK_REPOSITORY:-ArcielB/runwayclock}"
requested_version="latest"
widget_mode="auto"

usage() {
    printf '%s\n' \
        "Install or update RunwayClock for the current Linux user." \
        "" \
        "Usage: ./install.sh [--version vX.Y.Z] [--widget|--no-widget]" \
        "" \
        "The installer downloads a GitHub release, verifies every asset against" \
        "SHA256SUMS, extracts the AppImage without requiring FUSE, and creates" \
        "desktop-menu and command-line launchers. Financial data is never changed."
}

while (( $# > 0 )); do
    case "$1" in
        --version)
            [[ $# -ge 2 ]] || { echo "--version requires a value" >&2; exit 2; }
            requested_version="$2"
            shift 2
            ;;
        --widget)
            widget_mode="yes"
            shift
            ;;
        --no-widget)
            widget_mode="no"
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

for command_name in curl sha256sum sed awk grep tar mktemp; do
    command -v "$command_name" >/dev/null || {
        echo "RunwayClock requires '$command_name' to install." >&2
        exit 1
    }
done

case "$(uname -m)" in
    x86_64|amd64)
        architecture_pattern='(amd64|x86_64)\.AppImage$'
        ;;
    aarch64|arm64)
        architecture_pattern='(aarch64|arm64)\.AppImage$'
        ;;
    *)
        echo "No RunwayClock release is available for architecture $(uname -m)." >&2
        echo "Build from source using the instructions in CONTRIBUTING.md." >&2
        exit 1
        ;;
esac

if [[ -n "${RUNWAYCLOCK_RELEASE_API:-}" ]]; then
    release_api="$RUNWAYCLOCK_RELEASE_API"
elif [[ "$requested_version" == "latest" ]]; then
    release_api="https://api.github.com/repos/$repository/releases/latest"
else
    release_api="https://api.github.com/repos/$repository/releases/tags/$requested_version"
fi

temporary_dir="$(mktemp -d)"
cleanup() {
    rm -rf -- "$temporary_dir"
}
trap cleanup EXIT

download() {
    local url="$1"
    local destination="$2"
    curl --fail --location --silent --show-error \
        --retry 3 --retry-delay 1 \
        --header 'Accept: application/vnd.github+json' \
        --user-agent 'RunwayClock-installer' \
        --output "$destination" "$url"
}

echo "Finding RunwayClock $requested_version release…"
release_json="$temporary_dir/release.json"
if ! download "$release_api" "$release_json"; then
    echo "Could not find a published RunwayClock release." >&2
    echo "Release API: $release_api" >&2
    exit 1
fi

tag_name="$(sed -n 's/.*"tag_name": "\([^"]*\)".*/\1/p' "$release_json" | head -n 1)"
asset_urls="$(sed -n 's/.*"browser_download_url": "\([^"]*\)".*/\1/p' "$release_json")"
if [[ -z "$tag_name" || -z "$asset_urls" ]]; then
    echo "The GitHub release response did not contain usable assets." >&2
    exit 1
fi

find_asset() {
    local pattern="$1"
    local url
    while IFS= read -r url; do
        if [[ "${url##*/}" =~ $pattern ]]; then
            printf '%s\n' "$url"
            return 0
        fi
    done <<< "$asset_urls"
    return 1
}

appimage_url="$(find_asset "$architecture_pattern" || true)"
checksums_url="$(find_asset '^SHA256SUMS$' || true)"
icon_url="$(find_asset '^runwayclock\.png$' || true)"
installer_url="$(find_asset '^runwayclock-installer\.sh$' || true)"
uninstaller_url="$(find_asset '^runwayclock-uninstall\.sh$' || true)"
widget_url="$(find_asset '^runwayclock-gnome-widget\.tar\.gz$' || true)"

for required_url in \
    "$appimage_url" "$checksums_url" "$icon_url" \
    "$installer_url" "$uninstaller_url"; do
    if [[ -z "$required_url" ]]; then
        echo "Release $tag_name is missing a required installation asset." >&2
        exit 1
    fi
done

download_asset() {
    local url="$1"
    local name="${url##*/}"
    if [[ ! "$name" =~ ^[A-Za-z0-9._+-]+$ ]]; then
        echo "Refusing unsafe release asset name: $name" >&2
        exit 1
    fi
    download "$url" "$temporary_dir/$name"
}

download_asset "$checksums_url"
download_asset "$appimage_url"
download_asset "$icon_url"
download_asset "$installer_url"
download_asset "$uninstaller_url"

install_widget="no"
if [[ "$widget_mode" == "yes" ]] \
    || [[ "$widget_mode" == "auto" && -n "$(command -v gnome-shell || true)" ]]; then
    install_widget="yes"
    if [[ -z "$widget_url" ]]; then
        echo "Warning: this release has no GNOME widget asset; continuing without it." >&2
        install_widget="no"
    else
        download_asset "$widget_url"
    fi
fi

verify_asset() {
    local name="$1"
    local checksum_line
    checksum_line="$(awk -v file="$name" '$2 == file || $2 == "*" file { print; exit }' \
        "$temporary_dir/SHA256SUMS")"
    if [[ -z "$checksum_line" ]]; then
        echo "SHA256SUMS has no entry for $name." >&2
        exit 1
    fi
    printf '%s\n' "$checksum_line" | (cd "$temporary_dir" && sha256sum --check -)
}

appimage_name="${appimage_url##*/}"
verify_asset "$appimage_name"
verify_asset "runwayclock.png"
verify_asset "runwayclock-installer.sh"
verify_asset "runwayclock-uninstall.sh"
if [[ "$install_widget" == "yes" ]]; then
    verify_asset "runwayclock-gnome-widget.tar.gz"
fi

chmod +x "$temporary_dir/$appimage_name"
mkdir -p "$temporary_dir/extracted"
(
    cd "$temporary_dir/extracted"
    "$temporary_dir/$appimage_name" --appimage-extract >/dev/null
)
extracted_app="$temporary_dir/extracted/squashfs-root"
if [[ ! -x "$extracted_app/AppRun" ]]; then
    echo "The RunwayClock AppImage did not contain an executable AppRun." >&2
    exit 1
fi

data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
bin_home="${RUNWAYCLOCK_BIN_DIR:-$HOME/.local/bin}"
application_home="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
install_root="${RUNWAYCLOCK_INSTALL_ROOT:-$data_home/runwayclock/product}"
desktop_file="$application_home/app.runwayclock.desktop.desktop"

case "$install_root" in
    ""|/|"$HOME"|"$data_home"|"$data_home/runwayclock")
        echo "Refusing unsafe installation directory: $install_root" >&2
        exit 1
        ;;
esac
if [[ -L "$install_root" ]]; then
    echo "Refusing to replace symlinked installation directory: $install_root" >&2
    exit 1
fi

install_parent="$(dirname -- "$install_root")"
next_install="$install_root.new.$$"
previous_install="$install_root.previous.$$"
mkdir -p "$install_parent" "$bin_home" "$application_home"
mkdir -p "$next_install"
cp -a "$extracted_app/." "$next_install/"
install -m 0644 "$temporary_dir/runwayclock.png" "$next_install/runwayclock.png"
install -m 0755 "$temporary_dir/runwayclock-installer.sh" "$next_install/install.sh"
install -m 0755 "$temporary_dir/runwayclock-uninstall.sh" "$next_install/uninstall.sh"
printf '#!/usr/bin/env bash\nexec %q "$@"\n' "$install_root/AppRun" \
    > "$next_install/runwayclock-app"
chmod 0755 "$next_install/runwayclock-app"
printf '%s\n' "$tag_name" > "$next_install/VERSION"

if [[ -d "$install_root" ]]; then
    mv "$install_root" "$previous_install"
fi
if ! mv "$next_install" "$install_root"; then
    [[ ! -d "$previous_install" ]] || mv "$previous_install" "$install_root"
    echo "Could not activate the new RunwayClock installation." >&2
    exit 1
fi
[[ ! -d "$previous_install" ]] || rm -rf -- "$previous_install"

ln -sfn "$install_root/runwayclock-app" "$bin_home/runwayclock-app"
ln -sfn "$install_root/install.sh" "$bin_home/runwayclock-update"
ln -sfn "$install_root/uninstall.sh" "$bin_home/runwayclock-uninstall"

escaped_exec="${install_root//\\/\\\\}"
escaped_exec="${escaped_exec//\"/\\\"}"
escaped_exec="${escaped_exec//\$/\\\$}"
escaped_exec="${escaped_exec//\`/\\\`}"
desktop_temporary="$temporary_dir/app.runwayclock.desktop.desktop"
cat > "$desktop_temporary" <<EOF
[Desktop Entry]
Type=Application
Name=RunwayClock
Comment=Know how long your money can sustain you
Exec="$escaped_exec/AppRun"
Icon=$install_root/runwayclock.png
Terminal=false
Categories=Office;Finance;
StartupNotify=true
StartupWMClass=RunwayClock
X-RunwayClock-Version=$tag_name
EOF
install -m 0644 "$desktop_temporary" "$desktop_file"
if command -v update-desktop-database >/dev/null; then
    update-desktop-database "$application_home" >/dev/null 2>&1 || true
fi

if [[ "$install_widget" == "yes" ]]; then
    mkdir -p "$temporary_dir/widget"
    tar -xzf "$temporary_dir/runwayclock-gnome-widget.tar.gz" \
        -C "$temporary_dir/widget"
    if ! bash "$temporary_dir/widget/widgets/gnome/install.sh"; then
        echo "Warning: RunwayClock installed, but the optional GNOME widget did not." >&2
    fi
fi

echo
echo "RunwayClock $tag_name is installed."
echo "Open it from your application menu or run: $bin_home/runwayclock-app"
echo "Update later with: $bin_home/runwayclock-update"
echo "Uninstall with: $bin_home/runwayclock-uninstall"
if [[ ":$PATH:" != *":$bin_home:"* ]]; then
    echo "Add $bin_home to PATH if you want to use those terminal commands by name."
fi
