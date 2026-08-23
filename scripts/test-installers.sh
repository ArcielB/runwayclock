#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
test_root="$(mktemp -d)"
cleanup() {
    rm -rf -- "$test_root"
}
trap cleanup EXIT

assets="$test_root/assets"
mkdir -p "$assets"

case "$(uname -m)" in
    x86_64|amd64) release_arch="amd64" ;;
    aarch64|arm64) release_arch="aarch64" ;;
    *) echo "Unsupported installer-test architecture: $(uname -m)" >&2; exit 1 ;;
esac
appimage_asset="RunwayClock_0.1.0_${release_arch}.AppImage"

cat > "$assets/$appimage_asset" <<'APPIMAGE'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" != "--appimage-extract" ]]; then
    echo "fake AppImage only supports extraction" >&2
    exit 1
fi
mkdir -p squashfs-root
cat > squashfs-root/AppRun <<'APPRUN'
#!/usr/bin/env bash
printf '%s\n' "RunwayClock test application"
APPRUN
chmod +x squashfs-root/AppRun
APPIMAGE
chmod +x "$assets/$appimage_asset"

printf 'synthetic icon\n' > "$assets/runwayclock.png"
cp "$repository_root/install.sh" "$assets/runwayclock-installer.sh"
cp "$repository_root/uninstall.sh" "$assets/runwayclock-uninstall.sh"
(
    cd "$assets"
    sha256sum \
        "$appimage_asset" \
        runwayclock.png \
        runwayclock-installer.sh \
        runwayclock-uninstall.sh > SHA256SUMS
)

release_json="$test_root/release.json"
asset_url="file://$assets"
cat > "$release_json" <<EOF
{
  "tag_name": "v0.1.0",
  "assets": [
    {"browser_download_url": "$asset_url/$appimage_asset"},
    {"browser_download_url": "$asset_url/runwayclock.png"},
    {"browser_download_url": "$asset_url/runwayclock-installer.sh"},
    {"browser_download_url": "$asset_url/runwayclock-uninstall.sh"},
    {"browser_download_url": "$asset_url/SHA256SUMS"}
  ]
}
EOF

test_home="$test_root/home"
test_data="$test_home/data"
test_bin="$test_home/bin"
mkdir -p "$test_home"

run_installer() {
    HOME="$test_home" \
    XDG_DATA_HOME="$test_data" \
    RUNWAYCLOCK_BIN_DIR="$test_bin" \
    RUNWAYCLOCK_RELEASE_API="file://$release_json" \
        bash "$repository_root/install.sh" --no-widget
}

run_installer
run_installer

test -x "$test_data/runwayclock/product/AppRun"
test -L "$test_bin/runwayclock-app"
test -L "$test_bin/runwayclock-update"
test -L "$test_bin/runwayclock-uninstall"
test "$("$test_bin/runwayclock-app")" = "RunwayClock test application"
test "$(< "$test_data/runwayclock/product/VERSION")" = "v0.1.0"
grep -Fq "Exec=\"$test_data/runwayclock/product/AppRun\"" \
    "$test_data/applications/app.runwayclock.desktop.desktop"

printf 'preserve me\n' > "$test_data/runwayclock/runwayclock.db"
HOME="$test_home" \
XDG_DATA_HOME="$test_data" \
RUNWAYCLOCK_BIN_DIR="$test_bin" \
    "$test_bin/runwayclock-uninstall" --keep-widget
test ! -e "$test_data/runwayclock/product"
test -f "$test_data/runwayclock/runwayclock.db"

HOME="$test_home" \
XDG_DATA_HOME="$test_data" \
RUNWAYCLOCK_BIN_DIR="$test_bin" \
    bash "$repository_root/uninstall.sh" --keep-widget --purge-data
test ! -e "$test_data/runwayclock"

fake_bin="$test_root/fake-bin"
mkdir -p "$fake_bin"
cat > "$fake_bin/gnome-shell" <<'EOF'
#!/usr/bin/env bash
echo "GNOME Shell ${FAKE_GNOME_VERSION:?}"
EOF
cat > "$fake_bin/gnome-extensions" <<'EOF'
#!/usr/bin/env bash
case "${1:-}" in
    list) exit 0 ;;
    enable|disable) exit 0 ;;
esac
EOF
cat > "$fake_bin/gsettings" <<'EOF'
#!/usr/bin/env bash
if [[ "${1:-}" == "get" ]]; then
    echo "@as []"
fi
EOF
chmod +x "$fake_bin/gnome-shell" "$fake_bin/gnome-extensions" "$fake_bin/gsettings"

legacy_data="$test_root/legacy-data"
PATH="$fake_bin:$PATH" \
HOME="$test_home" \
XDG_DATA_HOME="$legacy_data" \
FAKE_GNOME_VERSION="42.9" \
    bash "$repository_root/widgets/gnome/install.sh"
cmp "$repository_root/widgets/gnome/runwayclock@runwayclock.local/extension.js" \
    "$legacy_data/gnome-shell/extensions/runwayclock@runwayclock.local/extension.js"

modern_data="$test_root/modern-data"
PATH="$fake_bin:$PATH" \
HOME="$test_home" \
XDG_DATA_HOME="$modern_data" \
FAKE_GNOME_VERSION="46.2" \
    bash "$repository_root/widgets/gnome/install.sh"
cmp "$repository_root/widgets/gnome/runwayclock@runwayclock.local/extension-modern.js" \
    "$modern_data/gnome-shell/extensions/runwayclock@runwayclock.local/extension.js"
cmp "$repository_root/widgets/gnome/runwayclock@runwayclock.local/metadata-modern.json" \
    "$modern_data/gnome-shell/extensions/runwayclock@runwayclock.local/metadata.json"

fake_bundles="$test_root/bundles"
mkdir -p "$fake_bundles/appimage" "$fake_bundles/deb" "$fake_bundles/rpm"
printf 'appimage\n' > "$fake_bundles/appimage/RunwayClock_0.1.0_amd64.AppImage"
printf 'deb\n' > "$fake_bundles/deb/runwayclock-app_0.1.0_amd64.deb"
printf 'rpm\n' > "$fake_bundles/rpm/runwayclock-app-0.1.0-1.x86_64.rpm"
release_output="$test_root/release-output"
RUNWAYCLOCK_BUNDLE_ROOT="$fake_bundles" \
    "$repository_root/scripts/package-release-assets.sh" "$release_output"
(
    cd "$release_output"
    sha256sum --check SHA256SUMS
)
tar -tzf "$release_output/runwayclock-gnome-widget.tar.gz" \
    | grep -Fq 'widgets/gnome/install.sh'

echo "Installer and GNOME widget tests passed."
