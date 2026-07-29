#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
resolver="$script_dir/resolve-version.sh"

expect_valid() {
  local tag="$1"
  local version="$2"
  local deb_version="$3"
  local output

  output="$("$resolver" "$tag")"
  grep -Fxq "version=$version" <<< "$output"
  grep -Fxq "deb_version=$deb_version" <<< "$output"
  grep -Fxq "tag=$tag" <<< "$output"
}

expect_invalid() {
  local tag="$1"

  if "$resolver" "$tag" >/dev/null 2>&1; then
    echo "Expected version to be rejected: $tag" >&2
    exit 1
  fi
}

expect_valid "gtty-v0.1.0" "0.1.0" "0.1.0"
expect_valid "gtty-v1.2.3-rc.1" "1.2.3-rc.1" "1.2.3~rc.1"
expect_valid \
  "gtty-v1.2.3-beta.2+build.7" \
  "1.2.3-beta.2+build.7" \
  "1.2.3~beta.2+build.7"
expect_valid \
  "gtty-v1.2.3+build-7" \
  "1.2.3+build-7" \
  "1.2.3+build-7"

expect_invalid "v1.2.3"
expect_invalid "gtty-v1.2"
expect_invalid "gtty-v01.2.3"
expect_invalid "gtty-v1.2.3.4"
expect_invalid "gtty-v1.2.3-01"
expect_invalid "gtty-v1.2.3-"
expect_invalid "gtty-v1.2.3+"

echo "GTTY release version tests passed."
