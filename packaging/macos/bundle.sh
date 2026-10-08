#!/bin/bash
# Build ytmfast.app on macOS: bundle.sh <binary> <output.app> <version>
# Set CODESIGN_IDENTITY for Developer ID signing; otherwise sign ad-hoc.
set -euo pipefail

binary="$1"
app="$2"
version="$3"
here="$(cd "$(dirname "$0")" && pwd)"

mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/ytmfast"
chmod 755 "$app/Contents/MacOS/ytmfast"
# Branch names from workflow_dispatch are labels, not numeric build numbers.
build="$version"
if [[ ! "$build" =~ ^[0-9]+(\.[0-9]+)*$ ]]; then
    build=0
fi
# The version is a label; keep only characters that are safe in XML and sed.
version="${version//[^A-Za-z0-9._-]/-}"
sed -e "s|__VERSION__|$version|g" -e "s|__BUILD__|$build|g" \
    "$here/Info.plist" > "$app/Contents/Info.plist"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
iconset="$tmp/ytmfast.iconset"
mkdir -p "$iconset"
# iconutil supports these base sizes and their @2x variants.
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$here/../../assets/app-icon.png" \
        --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$here/../../assets/app-icon.png" \
        --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/ytmfast.icns"

if [ -n "${CODESIGN_IDENTITY:-}" ]; then
    codesign --force --timestamp --options runtime --sign "$CODESIGN_IDENTITY" "$app"
else
    codesign --force --sign - "$app"
fi
codesign --verify --strict "$app"

echo "$app"
