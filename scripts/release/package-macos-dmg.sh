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
  arm64) ;;
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

original_entitlements="$work_dir/original-entitlements.plist"
codesign -d --entitlements :- "$packaged_app" \
  > "$original_entitlements" 2>/dev/null
test -s "$original_entitlements"

bundle_version="${version%%[-+]*}"
plist="$packaged_app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleName GTTY" "$plist" \
  || /usr/libexec/PlistBuddy -c "Add :CFBundleName string GTTY" "$plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName GTTY" "$plist" \
  || /usr/libexec/PlistBuddy -c "Add :CFBundleDisplayName string GTTY" "$plist"
/usr/libexec/PlistBuddy -c \
  "Set :CFBundleShortVersionString $bundle_version" "$plist" \
  || /usr/libexec/PlistBuddy -c \
    "Add :CFBundleShortVersionString string $bundle_version" "$plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $bundle_version" "$plist" \
  || /usr/libexec/PlistBuddy -c \
    "Add :CFBundleVersion string $bundle_version" "$plist"
/usr/libexec/PlistBuddy -c "Add :GTTYUpdatesEnabled bool false" "$plist" \
  || /usr/libexec/PlistBuddy -c "Set :GTTYUpdatesEnabled false" "$plist"
/usr/libexec/PlistBuddy -c "Delete :SUPublicEDKey" "$plist" || true

dock_plugin="$(
  /usr/libexec/PlistBuddy -c 'Print :NSDockTilePlugIn' "$plist" 2>/dev/null \
    || true
)"
if [[ -n "$dock_plugin" ]]; then
  while IFS= read -r -d '' plugin_path; do
    rm -rf -- "$plugin_path"
  done < <(
    find "$packaged_app/Contents" -type d -name "$dock_plugin" -prune -print0
  )
  /usr/libexec/PlistBuddy -c "Delete :NSDockTilePlugIn" "$plist"
fi

while IFS= read -r -d '' bundle_plist; do
  identifier="$(
    /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \
      "$bundle_plist" 2>/dev/null || true
  )"
  case "$identifier" in
    com.mitchellh.ghostty*)
      gtty_identifier="com.yuebanddd.gtty${identifier#com.mitchellh.ghostty}"
      /usr/libexec/PlistBuddy -c \
        "Set :CFBundleIdentifier $gtty_identifier" "$bundle_plist"
      ;;
  esac
done < <(find "$packaged_app" -type f -name Info.plist -print0)

codesign \
  --force \
  --deep \
  --sign - \
  --timestamp=none \
  --preserve-metadata=entitlements,flags \
  "$packaged_app"
codesign --verify --deep --strict "$packaged_app"
signed_entitlements="$work_dir/signed-entitlements.plist"
codesign -d --entitlements :- "$packaged_app" \
  > "$signed_entitlements" 2>/dev/null
cmp "$original_entitlements" "$signed_entitlements"
codesign -d --verbose=4 "$packaged_app" 2>&1 | grep -Eq 'flags=.*runtime'
ln -s /Applications "$staging_dir/Applications"

artifact="$output_dir/GTTY-${version}-macOS-${architecture}-unsigned.dmg"
hdiutil create \
  -volname "GTTY" \
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
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \
  "$mount_dir/GTTY.app/Contents/Info.plist")" = "com.yuebanddd.gtty"
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' \
  "$mount_dir/GTTY.app/Contents/Info.plist")" = "$bundle_version"
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' \
  "$mount_dir/GTTY.app/Contents/Info.plist")" = "$bundle_version"
test "$(/usr/libexec/PlistBuddy -c 'Print :GTTYUpdatesEnabled' \
  "$mount_dir/GTTY.app/Contents/Info.plist")" = "false"
if /usr/libexec/PlistBuddy -c 'Print :SUPublicEDKey' \
  "$mount_dir/GTTY.app/Contents/Info.plist" >/dev/null 2>&1; then
  echo "The packaged app retains the upstream Sparkle public key." >&2
  exit 1
fi
if /usr/libexec/PlistBuddy -c 'Print :NSDockTilePlugIn' \
  "$mount_dir/GTTY.app/Contents/Info.plist" >/dev/null 2>&1; then
  echo "The packaged app retains the incompatible Dock tile plugin." >&2
  exit 1
fi
while IFS= read -r -d '' bundle_plist; do
  identifier="$(
    /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \
      "$bundle_plist" 2>/dev/null || true
  )"
  if [[ "$identifier" == com.mitchellh.ghostty* ]]; then
    echo "The packaged app retains an upstream bundle ID: $identifier" >&2
    exit 1
  fi
done < <(find "$mount_dir/GTTY.app" -type f -name Info.plist -print0)
codesign --verify --deep --strict "$mount_dir/GTTY.app"
mounted_entitlements="$work_dir/mounted-entitlements.plist"
codesign -d --entitlements :- "$mount_dir/GTTY.app" \
  > "$mounted_entitlements" 2>/dev/null
cmp "$original_entitlements" "$mounted_entitlements"
codesign -d --verbose=4 "$mount_dir/GTTY.app" 2>&1 \
  | grep -Eq 'flags=.*runtime'
test -s "$artifact"

hdiutil detach "$device" -quiet
device=""
echo "Created $artifact"
