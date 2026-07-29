#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 6 ]]; then
  echo "usage: $0 VERSION DEB_VERSION ARCH INSTALL_ROOT GTTYD_BINARY OUTPUT_DIR" >&2
  exit 2
fi

version="$1"
deb_version="$2"
architecture="$3"
install_root="$4"
gttyd_binary="$5"
output_dir="$6"

case "$architecture" in
  amd64 | arm64) ;;
  *)
    echo "Unsupported Debian architecture: $architecture" >&2
    exit 1
    ;;
esac

ghostty_binary="$install_root/usr/bin/ghostty"
if [[ ! -x "$ghostty_binary" ]]; then
  echo "Ghostty executable is missing from the install root: $ghostty_binary" >&2
  exit 1
fi
if [[ ! -x "$gttyd_binary" ]]; then
  echo "gttyd executable is missing: $gttyd_binary" >&2
  exit 1
fi

work_dir="$(mktemp -d)"
trap 'rm -rf -- "$work_dir"' EXIT

package_root="$work_dir/debian/gtty"
mkdir -p "$package_root"
cp -a "$install_root/." "$package_root/"
install -Dm755 "$gttyd_binary" "$package_root/usr/bin/gttyd"
ln -s ghostty "$package_root/usr/bin/gtty"

staging_prefix="$install_root/usr"
while IFS= read -r -d '' file; do
  sed -i "s|$staging_prefix|/usr|g" "$file"
done < <(grep -rIlZ --fixed-strings "$staging_prefix" "$package_root/usr" || true)

if grep -rIl --fixed-strings "$staging_prefix" "$package_root/usr" | grep -q .; then
  echo "The Debian package still contains its temporary build prefix." >&2
  exit 1
fi

cat > "$work_dir/debian/control" <<EOF
Source: gtty
Section: utils
Priority: optional
Maintainer: GTTY Project <noreply@github.com>
Standards-Version: 4.7.2

Package: gtty
Architecture: $architecture
Description: GTTY AI development terminal preview
 GTTY combines the Ghostty terminal foundation with a local AI task daemon.
EOF

shlibdeps_args=(
  -O
  -l"debian/gtty/usr/lib"
  -l"debian/gtty/usr/lib/gtty"
  -e"debian/gtty/usr/bin/ghostty"
  -e"debian/gtty/usr/bin/gttyd"
)
private_layer_shell="$package_root/usr/lib/gtty/libgtk4-layer-shell.so.0"
if [[ -e "$private_layer_shell" ]]; then
  private_shlibs="$work_dir/debian/gtty.shlibs.local"
  printf 'libgtk4-layer-shell 0 gtty (= %s)\n' "$deb_version" \
    > "$private_shlibs"
  shlibdeps_args+=("-Ldebian/gtty.shlibs.local" -xgtty)
fi

dependency_output="$(
  cd "$work_dir"
  dpkg-shlibdeps "${shlibdeps_args[@]}"
)"
dependencies="${dependency_output#shlibs:Depends=}"
if [[ -z "$dependencies" || "$dependencies" == "$dependency_output" ]]; then
  echo "Unable to derive Debian runtime dependencies." >&2
  exit 1
fi

installed_size="$(du -sk "$package_root" | awk '{print $1}')"
mkdir -p "$package_root/DEBIAN"
cat > "$package_root/DEBIAN/control" <<EOF
Package: gtty
Version: $deb_version
Section: utils
Priority: optional
Architecture: $architecture
Maintainer: GTTY Project <noreply@github.com>
Installed-Size: $installed_size
Depends: $dependencies
Provides: ghostty
Conflicts: ghostty
Replaces: ghostty
Homepage: https://github.com/yuebanddd/ghostty
Description: GTTY AI development terminal preview
 GTTY combines the Ghostty terminal foundation with the gttyd local AI task
 daemon. This unsigned preview package is built for Ubuntu 24.04 or compatible
 Debian-based distributions.
EOF

cat > "$package_root/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
command -v update-desktop-database >/dev/null 2>&1 \
  && update-desktop-database -q /usr/share/applications || true
command -v gtk-update-icon-cache >/dev/null 2>&1 \
  && gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
EOF
chmod 0755 "$package_root/DEBIAN/postinst"

cat > "$package_root/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
command -v update-desktop-database >/dev/null 2>&1 \
  && update-desktop-database -q /usr/share/applications || true
command -v gtk-update-icon-cache >/dev/null 2>&1 \
  && gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
EOF
chmod 0755 "$package_root/DEBIAN/postrm"

mkdir -p "$output_dir"
artifact="$output_dir/gtty_${version}_${architecture}.deb"
dpkg-deb --root-owner-group --build "$package_root" "$artifact"

test "$(dpkg-deb --field "$artifact" Package)" = "gtty"
test "$(dpkg-deb --field "$artifact" Version)" = "$deb_version"
test "$(dpkg-deb --field "$artifact" Architecture)" = "$architecture"
dpkg-deb --contents "$artifact" | grep -Eq '[.]/usr/bin/(gtty|ghostty)$'
dpkg-deb --contents "$artifact" | grep -Eq '[.]/usr/bin/gttyd$'
if [[ -e "$private_layer_shell" ]]; then
  dpkg-deb --contents "$artifact" \
    | grep -Eq '[.]/usr/lib/gtty/libgtk4-layer-shell[.]so[.]0$'
fi
test -s "$artifact"

echo "Created $artifact"
