#!/bin/sh
# Builds dist/Osiris.app. Put your PharaohData folder next to the app (or in
# ~/Library/Application Support/Osiris/) to play.
set -eu
cd "$(dirname "$0")/../.."
. "$HOME/.cargo/env" 2>/dev/null || true

cargo build --release -p osiris-app

APP=dist/Osiris.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/osiris "$APP/Contents/MacOS/Osiris"

# Icon: round the generated artwork's corners, then build the .icns set.
ICONSET=$(mktemp -d)/Osiris.iconset
mkdir -p "$ICONSET"
magick packaging/macos/icon-source.png -resize 1024x1024 \
  \( -size 1024x1024 xc:none -fill white -draw "roundrectangle 0,0 1023,1023 190,190" \) \
  -compose DstIn -composite /tmp/osiris-icon-1024.png
for s in 16 32 128 256 512; do
  magick /tmp/osiris-icon-1024.png -resize ${s}x${s} "$ICONSET/icon_${s}x${s}.png"
  magick /tmp/osiris-icon-1024.png -resize $((s*2))x$((s*2)) "$ICONSET/icon_${s}x${s}@2x.png"
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Osiris.icns"

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Osiris</string>
  <key>CFBundleDisplayName</key><string>Osiris</string>
  <key>CFBundleIdentifier</key><string>org.osiris-engine.osiris</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>Osiris</string>
  <key>CFBundleIconFile</key><string>Osiris</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.strategy-games</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$APP" 2>/dev/null || true
echo "Built $APP"
