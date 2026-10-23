#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

profile=release
if [[ "${1:-}" == "--debug" ]]; then profile=debug; fi
if [[ "$profile" == release ]]; then
    cargo build --release --locked
else
    cargo build --locked
fi

bundle="dist/Hours.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources" dist/Hours.iconset
cp "target/$profile/hours-rs" "$bundle/Contents/MacOS/hours-rs"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" assets/app-icon.png --out "dist/Hours.iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" assets/app-icon.png --out "dist/Hours.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns dist/Hours.iconset -o "$bundle/Contents/Resources/Hours.icns"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Hours</string>
  <key>CFBundleDisplayName</key><string>Hours</string>
  <key>CFBundleIdentifier</key><string>com.chrisvo.hours-rs</string>
  <key>CFBundleExecutable</key><string>hours-rs</string>
  <key>CFBundleIconFile</key><string>Hours.icns</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --deep --sign - "$bundle"
ditto -c -k --sequesterRsrc --keepParent "$bundle" dist/Hours-macOS.zip
printf 'Built %s\n' "$PWD/$bundle"
