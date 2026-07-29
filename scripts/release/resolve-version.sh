#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 1 ]]; then
  echo "usage: $0 gtty-v<semantic-version>" >&2
  exit 2
fi

tag="$1"
if [[ "$tag" != gtty-v* ]]; then
  echo "GTTY release tags must start with gtty-v: $tag" >&2
  exit 1
fi

version="${tag#gtty-v}"
identifier='[0-9A-Za-z-]+'
core='(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)'
prerelease="(-${identifier}(\.${identifier})*)?"
build="(\+${identifier}(\.${identifier})*)?"

if [[ ! "$version" =~ ^${core}${prerelease}${build}$ ]]; then
  echo "Invalid semantic version in GTTY tag: $tag" >&2
  exit 1
fi

without_build="${version%%+*}"
if [[ "$without_build" == *-* ]]; then
  prerelease_value="${without_build#*-}"
  IFS='.' read -r -a prerelease_identifiers <<< "$prerelease_value"
  for value in "${prerelease_identifiers[@]}"; do
    if [[ "$value" =~ ^[0-9]+$ && "$value" != "0" && "$value" == 0* ]]; then
      echo "Numeric prerelease identifiers cannot contain leading zeroes: $tag" >&2
      exit 1
    fi
  done
fi

deb_version="${version/-/\~}"

printf 'version=%s\n' "$version"
printf 'deb_version=%s\n' "$deb_version"
printf 'tag=%s\n' "$tag"
