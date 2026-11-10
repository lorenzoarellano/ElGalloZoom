#!/usr/bin/env bash
set -euo pipefail

APP_NAME="ELGalloZoom"
BUNDLE_ID="com.egz.elgallozoom"
VERSION="0.1.0"
BIN_NAME="el-gallo-zoom"

echo "==> [1/4] Compiling ${APP_NAME} (release)..."
cargo build --release

APP_DIR="target/release/bundle/${APP_NAME}.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

rm -rf "${APP_DIR}"
mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"

echo "==> [2/4] Creating ${APP_NAME}.app bundle..."
cp "target/release/${BIN_NAME}" "${MACOS_DIR}/${BIN_NAME}"
chmod +x "${MACOS_DIR}/${BIN_NAME}"

# Convert assets/logo.png to macOS .icns using native sips + iconutil
if [ -f "assets/logo.png" ]; then
    ICONSET_DIR="target/release/bundle/AppIcon.iconset"
    rm -rf "${ICONSET_DIR}"
    mkdir -p "${ICONSET_DIR}"
    sips -z 16 16     "assets/logo.png" --out "${ICONSET_DIR}/icon_16x16.png"      >/dev/null
    sips -z 32 32     "assets/logo.png" --out "${ICONSET_DIR}/icon_16x16@2x.png"   >/dev/null
    sips -z 32 32     "assets/logo.png" --out "${ICONSET_DIR}/icon_32x32.png"      >/dev/null
    sips -z 64 64     "assets/logo.png" --out "${ICONSET_DIR}/icon_32x32@2x.png"   >/dev/null
    sips -z 128 128   "assets/logo.png" --out "${ICONSET_DIR}/icon_128x128.png"    >/dev/null
    sips -z 256 256   "assets/logo.png" --out "${ICONSET_DIR}/icon_128x128@2x.png" >/dev/null
    sips -z 256 256   "assets/logo.png" --out "${ICONSET_DIR}/icon_256x256.png"    >/dev/null
    sips -z 512 512   "assets/logo.png" --out "${ICONSET_DIR}/icon_256x256@2x.png" >/dev/null
    iconutil -c icns "${ICONSET_DIR}" -o "${RESOURCES_DIR}/AppIcon.icns"
    rm -rf "${ICONSET_DIR}"
fi

cat > "${CONTENTS_DIR}/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>ELGalloZoom (EGZ)</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleExecutable</key>
    <string>${BIN_NAME}</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

echo "==> [3/4] Ad-hoc signing ${APP_NAME}.app..."
codesign --force --deep --sign - "${APP_DIR}"

DMG_STAGE="target/release/bundle/dmg_stage"
DMG_OUT="target/release/${APP_NAME}.dmg"
rm -rf "${DMG_STAGE}" "${DMG_OUT}"
mkdir -p "${DMG_STAGE}"
cp -R "${APP_DIR}" "${DMG_STAGE}/"
ln -s /Applications "${DMG_STAGE}/Applications"

echo "==> [4/4] Packaging ${DMG_OUT}..."
hdiutil create -volname "${APP_NAME}" -srcfolder "${DMG_STAGE}" -ov -format UDZO "${DMG_OUT}"
rm -rf "${DMG_STAGE}"

echo "✅ Done! Installer created at: ${DMG_OUT}"

