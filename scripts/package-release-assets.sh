#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
bundle_root="${RUNWAYCLOCK_BUNDLE_ROOT:-$repository_root/target/release/bundle}"
output_dir="${1:-$repository_root/dist-release}"

case "$output_dir" in
    ""|/|"$HOME"|"$repository_root")
        echo "Refusing unsafe release output directory: $output_dir" >&2
        exit 1
        ;;
esac

if [[ -e "$output_dir" ]]; then
    echo "Release output directory already exists: $output_dir" >&2
    exit 1
fi
mkdir -p "$output_dir"

copy_bundle_kind() {
    local pattern="$1"
    local label="$2"
    local found="no"
    local bundle
    while IFS= read -r -d '' bundle; do
        cp "$bundle" "$output_dir/"
        found="yes"
    done < <(find "$bundle_root" -type f -name "$pattern" -print0)
    if [[ "$found" != "yes" ]]; then
        echo "No $label bundle was found below $bundle_root." >&2
        exit 1
    fi
}

copy_bundle_kind '*.AppImage' 'AppImage'
copy_bundle_kind '*.deb' 'Debian'
copy_bundle_kind '*.rpm' 'RPM'

cp "$repository_root/app/src-tauri/icons/icon.png" "$output_dir/runwayclock.png"
cp "$repository_root/install.sh" "$output_dir/runwayclock-installer.sh"
cp "$repository_root/uninstall.sh" "$output_dir/runwayclock-uninstall.sh"
chmod +x \
    "$output_dir/runwayclock-installer.sh" \
    "$output_dir/runwayclock-uninstall.sh"

tar -czf "$output_dir/runwayclock-gnome-widget.tar.gz" \
    -C "$repository_root" widgets/gnome

mapfile -t release_files < <(
    find "$output_dir" -maxdepth 1 -type f -printf '%f\n' | LC_ALL=C sort
)
(
    cd "$output_dir"
    sha256sum "${release_files[@]}" > SHA256SUMS
)

echo "Packaged release assets in $output_dir:"
find "$output_dir" -maxdepth 1 -type f -printf '%f\n' | LC_ALL=C sort
