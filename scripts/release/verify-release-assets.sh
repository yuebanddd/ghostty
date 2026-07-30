#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 2 ]]; then
  echo "usage: $0 VERSION DIST_DIR" >&2
  exit 2
fi

version="$1"
dist_dir="$2"

expected=(
  "GTTY-${version}-macOS-arm64-unsigned.dmg"
  "gtty_${version}_amd64.deb"
  "gtty_${version}_arm64.deb"
)

for asset in "${expected[@]}"; do
  if [[ ! -s "$dist_dir/$asset" ]]; then
    echo "Required release asset is missing or empty: $asset" >&2
    exit 1
  fi
done

mapfile -t actual < <(
  find "$dist_dir" -maxdepth 1 -type f -printf '%f\n' | sort
)
mapfile -t expected_sorted < <(printf '%s\n' "${expected[@]}" | sort)

if [[ "${actual[*]}" != "${expected_sorted[*]}" ]]; then
  echo "Unexpected release asset set." >&2
  printf 'Expected: %s\n' "${expected_sorted[*]}" >&2
  printf 'Actual:   %s\n' "${actual[*]}" >&2
  exit 1
fi

echo "GTTY release asset set is complete."
