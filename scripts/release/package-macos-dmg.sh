#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 5 ]]; then
  echo "usage: $0 VERSION ARCH APP_BUNDLE GTTYD_BINARY OUTPUT_DIR" >&2
  exit 2
fi

version="$1"
architecture="$2"
app_bundle="$3"
gttyd_binary="$4"
output_dir="$5"

case "$architecture" in
  arm64 | x86_64) ;;
  *)
    echo "Unsupported macOS architecture: $architecture" >&2
    exit 1
    ;;
esac

if [[ ! -d "$app_bundle" ]]; then
  echo "macOS application bundle is missing: $app_bundle" >&2
  exit 1
fi
if [[ ! -x "$gttyd_binary" ]]; then
  echo "gttyd executable is missing: $gttyd_binary" >&2
  exit 1
fi

work_dir="$(mktemp -d)"
mount_dir="$work_dir/mount"
device=""

cleanup() {
  if [[ -n "$device" ]]; then
    hdiutil detach "$device" -quiet || true
  fi
  rm -rf -- "$work_dir"
}
trap cleanup EXIT

staging_dir="$work_dir/staging"
packaged_app="$staging_dir/GTTY.app"
mkdir -p "$staging_dir" "$mount_dir" "$output_dir"
ditto "$app_bundle" "$packaged_app"
install -m 0755 "$gttyd_binary" "$packaged_app/Contents/MacOS/gttyd"

plist="$packaged_app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleName GTTY" "$plist" \
  || /usr/libexec/PlistBuddy -c "Add :CFBundleName string GTTY" "$plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName GTTY" "$plist" \
  || /usr/libexec/PlistBuddy -c "Add :CFBundleDisplayName string GTTY" "$plist"

codesign --force --deep --sign - --timestamp=none "$packaged_app"
codesign --verify --deep --strict "$packaged_app"
ln -s /Applications "$staging_dir/Applications"

artifact="$output_dir/GTTY-${version}-macOS-${architecture}-unsigned.dmg"
hdiutil create \
  -volname "GTTY $version" \
  -srcfolder "$staging_dir" \
  -format UDZO \
  -ov \
  "$artifact"
hdiutil verify "$artifact"

attach_output="$work_dir/hdiutil-attach.txt"
hdiutil attach \
  "$artifact" \
  -nobrowse \
  -readonly \
  -mountpoint "$mount_dir" > "$attach_output"
device="$(awk '/^\/dev\// {print $1; exit}' "$attach_output")"
if [[ -z "$device" ]]; then
  echo "Unable to identify mounted DMG device." >&2
  exit 1
fi

test -d "$mount_dir/GTTY.app"
test -x "$mount_dir/GTTY.app/Contents/MacOS/gttyd"
test -L "$mount_dir/Applications"
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleName' \
  "$mount_dir/GTTY.app/Contents/Info.plist")" = "GTTY"
codesign --verify --deep --strict "$mount_dir/GTTY.app"
test -s "$artifact"

hdiutil detach "$device" -quiet
device=""
echo "Created $artifact"
