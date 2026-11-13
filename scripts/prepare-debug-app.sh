#!/usr/bin/env bash
# Wrap the cargo debug binary in a minimal .app so macOS shows the tray icon
# (raw binaries often get no NSStatusItem on recent macOS when launched via LLDB).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_NAME="rawlabelprint"
PRODUCT_NAME="RawLabelPrint"
IDENTIFIER="se.rawlabelprint.desktop"
TARGET_DIR="${ROOT}/src-tauri/target/debug"
SRC_BIN="${TARGET_DIR}/${BIN_NAME}"
APP_DIR="${TARGET_DIR}/${PRODUCT_NAME}.app"
CONTENTS="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS}/MacOS"
RESOURCES_DIR="${CONTENTS}/Resources"

if [[ ! -x "${SRC_BIN}" ]]; then
  echo "Missing debug binary: ${SRC_BIN}" >&2
  echo "Build first: cargo build --manifest-path src-tauri/Cargo.toml" >&2
  exit 1
fi

mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"
# Hardlink when possible so rebuilds stay fast; fall back to copy across volumes.
ln -f "${SRC_BIN}" "${MACOS_DIR}/${BIN_NAME}" 2>/dev/null || cp -f "${SRC_BIN}" "${MACOS_DIR}/${BIN_NAME}"

# Merge Tauri's custom Info.plist keys into a complete debug bundle plist.
cat > "${CONTENTS}/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>${BIN_NAME}</string>
  <key>CFBundleIdentifier</key>
  <string>${IDENTIFIER}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${PRODUCT_NAME}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.0.0-debug</string>
  <key>CFBundleVersion</key>
  <string>0.0.0-debug</string>
  <key>LSMinimumSystemVersion</key>
  <string>12.0</string>
  <key>LSUIElement</key>
  <true/>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSLocalNetworkUsageDescription</key>
  <string>RawLabelPrint discovers Zebra printers on your local network and sends label data directly to them.</string>
  <key>NSBonjourServices</key>
  <array>
    <string>_printer._tcp</string>
    <string>_ipp._tcp</string>
    <string>_http._tcp</string>
  </array>
</dict>
</plist>
EOF

echo "Debug app ready: ${APP_DIR}"
