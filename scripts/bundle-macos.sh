#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

profile=release
case "${1:-}" in
    "") ;;
    --debug) profile=dev ;;
    *) printf 'Usage: %s [--debug]\n' "$0" >&2; exit 1 ;;
esac
if [[ $# -gt 1 ]]; then
    printf 'Usage: %s [--debug]\n' "$0" >&2
    exit 1
fi
if [[ "$(uname -s)" != Darwin || "$(uname -m)" != arm64 ]]; then
    printf 'This script builds the Apple Silicon macOS app. Run it on an Apple Silicon Mac.\n' >&2
    exit 1
fi
if ! command -v cargo-about >/dev/null; then
    printf 'Install the notice generator with cargo install --locked --version 0.9.2 --features cli cargo-about\n' >&2
    exit 1
fi
rust_docs="$(rustc --print sysroot)/share/doc/rust"
if [[ ! -f "$rust_docs/COPYRIGHT-library.html" || ! -d "$rust_docs/licenses" ]]; then
    printf 'Install Rust license documents with rustup component add rust-docs\n' >&2
    exit 1
fi

export MACOSX_DEPLOYMENT_TARGET=12.0
mkdir -p dist
staging=$(mktemp -d "$PWD/dist/.bundle.XXXXXX")
trap 'rm -r "$staging"' EXIT
cargo metadata --locked --format-version 1 --filter-platform aarch64-apple-darwin > "$staging/metadata.json"
version=$(python3 - "$staging/metadata.json" <<'PY'
import json
import sys

metadata = json.load(open(sys.argv[1]))
package = next(p for p in metadata["packages"] if p["id"] == metadata["resolve"]["root"])
print(package["version"])
PY
)

bundle="$staging/Hours.app"
resources="$bundle/Contents/Resources"
mkdir -p "$bundle/Contents/MacOS" "$resources/licenses/egui-fonts" "$staging/Hours.iconset"
cargo about generate --locked --fail about.hbs -o "$resources/THIRD-PARTY-NOTICES.html"
cp LICENSE "$resources/LICENSE.txt"
cp assets/fonts/OFL-*.txt "$resources/licenses/"
mkdir -p "$resources/licenses/rust"
cp "$rust_docs/COPYRIGHT-library.html" "$resources/licenses/rust/"
cp -R "$rust_docs/licenses" "$resources/licenses/rust/"
python3 - "$staging/metadata.json" "$resources/licenses/egui-fonts" <<'PY'
import json
from pathlib import Path
import shutil
import sys

metadata = json.load(open(sys.argv[1]))
package = next(p for p in metadata["packages"] if p["name"] == "epaint_default_fonts")
fonts = Path(package["manifest_path"]).parent / "fonts"
for name in ("UFL.txt", "OFL.txt", "Hack-Regular.txt", "emoji-icon-font-mit-license.txt"):
    shutil.copy2(fonts / name, Path(sys.argv[2]) / name)
PY

cargo build --locked --profile "$profile" --message-format=json-render-diagnostics > "$staging/build.jsonl"
binary=$(python3 - "$staging/build.jsonl" <<'PY'
import json
import sys

artifacts = [json.loads(line) for line in open(sys.argv[1])]
print(next(a["executable"] for a in artifacts
           if a.get("reason") == "compiler-artifact"
           and a["target"]["name"] == "hours-rs" and a.get("executable")))
PY
)
if [[ "$(lipo -archs "$binary")" != arm64 ]]; then
    printf 'Expected an arm64 executable. Check your Cargo target configuration.\n' >&2
    exit 1
fi
cp "$binary" "$bundle/Contents/MacOS/hours-rs"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" assets/app-icon.png --out "$staging/Hours.iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" assets/app-icon.png --out "$staging/Hours.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$staging/Hours.iconset" -o "$resources/Hours.icns"
cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Hours</string>
  <key>CFBundleDisplayName</key><string>Hours</string>
  <key>CFBundleIdentifier</key><string>com.chrisvo.hours-rs</string>
  <key>CFBundleExecutable</key><string>hours-rs</string>
  <key>CFBundleIconFile</key><string>Hours.icns</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>$MACOSX_DEPLOYMENT_TARGET</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
plutil -lint "$bundle/Contents/Info.plist"
codesign --force --sign - "$bundle"
codesign --verify --deep --strict "$bundle"
asset="Hours-v${version}-macOS-arm64"
if [[ "${1:-}" == --debug ]]; then asset="$asset-debug"; fi
ditto -c -k --sequesterRsrc --keepParent "$bundle" "$staging/$asset.zip"
if [[ -d dist/Hours.app ]]; then rm -r dist/Hours.app; fi
mv "$bundle" dist/Hours.app
mv "$staging/$asset.zip" "dist/$asset.zip"
cp "dist/$asset.zip" dist/Hours-macOS.zip
(
    cd dist
    shasum -a 256 "$asset.zip" > "$asset.zip.sha256"
    shasum -a 256 -c "$asset.zip.sha256"
)
printf 'Built %s\nArchive: %s\n' "$PWD/dist/Hours.app" "$PWD/dist/$asset.zip"
