#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
tag="${1:-}"
if [[ ! "$tag" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
    echo "Release tag must use vMAJOR.MINOR.PATCH, received: ${tag:-<empty>}" >&2
    exit 1
fi
tag_version="${BASH_REMATCH[1]}"

workspace_version="$(awk '
    $0 == "[workspace.package]" { in_section = 1; next }
    /^\[/ { in_section = 0 }
    in_section && $1 == "version" {
        gsub(/["[:space:]]/, "", $3)
        print $3
        exit
    }
' "$repository_root/Cargo.toml")"
tauri_version="$(sed -n 's/^[[:space:]]*"version": "\([^"]*\)",/\1/p' \
    "$repository_root/app/src-tauri/tauri.conf.json" | head -n 1)"
npm_version="$(sed -n 's/^[[:space:]]*"version": "\([^"]*\)",/\1/p' \
    "$repository_root/app/package.json" | head -n 1)"

for version_pair in \
    "workspace:$workspace_version" \
    "tauri:$tauri_version" \
    "npm:$npm_version"; do
    source_name="${version_pair%%:*}"
    source_version="${version_pair#*:}"
    if [[ "$source_version" != "$tag_version" ]]; then
        echo "$source_name version $source_version does not match tag $tag." >&2
        exit 1
    fi
done

if [[ ! -s "$repository_root/LICENSE" ]]; then
    echo "A public release requires a non-empty LICENSE file." >&2
    exit 1
fi

echo "Release version $tag_version is consistent across Cargo, Tauri, and npm."
