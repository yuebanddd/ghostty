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
  if command -v dpkg >/dev/null 2>&1; then
    dpkg --validate-version "$deb_version"
  fi
}

expect_invalid() {
  local tag="$1"

  if "$resolver" "$tag" >/dev/null 2>&1; then
    echo "Expected version to be rejected: $tag" >&2
    exit 1
  fi
}

expect_distinct() {
  local first_tag="$1"
  local second_tag="$2"
  local first_version
  local second_version

  first_version="$("$resolver" "$first_tag" | sed -n 's/^deb_version=//p')"
  second_version="$("$resolver" "$second_tag" | sed -n 's/^deb_version=//p')"
  if [[ "$first_version" == "$second_version" ]]; then
    echo "Expected distinct Debian versions: $first_tag, $second_tag" >&2
    exit 1
  fi
  if command -v dpkg >/dev/null 2>&1 \
    && ! dpkg --compare-versions "$first_version" ne "$second_version"; then
    echo "dpkg considers distinct tags equal: $first_tag, $second_tag" >&2
    exit 1
  fi
}

expect_valid "gtty-v0.1.0" "0.1.0" "0.1.0"
expect_valid "gtty-v1.2.3-rc.1" "1.2.3-rc.1" "1.2.3~shcgd.n1"
expect_valid \
  "gtty-v1.2.3-beta.2+build.7" \
  "1.2.3-beta.2+build.7" \
  "1.2.3~sgcgfhegb.n2+bgchfgjgmgecodh"
expect_valid \
  "gtty-v1.2.3+build-7" \
  "1.2.3+build-7" \
  "1.2.3+bgchfgjgmgecndh"
expect_valid \
  "gtty-v1.2.3-alpha-" \
  "1.2.3-alpha-" \
  "1.2.3~sgbgmhagigbcn"
expect_valid \
  "gtty-v1.2.3-alpha-0" \
  "1.2.3-alpha-0" \
  "1.2.3~sgbgmhagigbcnda"
expect_valid \
  "gtty-v1.2.3+build-" \
  "1.2.3+build-" \
  "1.2.3+bgchfgjgmgecn"
expect_valid \
  "gtty-v1.2.3+build-0" \
  "1.2.3+build-0" \
  "1.2.3+bgchfgjgmgecnda"

expect_distinct "gtty-v1.2.3-alpha-" "gtty-v1.2.3-alpha-0"
expect_distinct "gtty-v1.2.3+build-" "gtty-v1.2.3+build-0"
expect_distinct "gtty-v1.2.3+build.0" "gtty-v1.2.3+build.00"
expect_distinct "gtty-v1.2.3-alpha1" "gtty-v1.2.3-alpha01"

expect_invalid "v1.2.3"
expect_invalid "gtty-v1.2"
expect_invalid "gtty-v01.2.3"
expect_invalid "gtty-v1.2.3.4"
expect_invalid "gtty-v1.2.3-01"
expect_invalid "gtty-v1.2.3-"
expect_invalid "gtty-v1.2.3+"

echo "GTTY release version tests passed."
