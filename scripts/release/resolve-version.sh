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

encode_bytes() {
  local value="$1"

  LC_ALL=C printf '%s' "$value" \
    | od -An -v -tx1 \
    | tr -d ' \n' \
    | tr '0123456789abcdef' 'cdefghijklmnopqr'
}

encode_prerelease() {
  local value="$1"
  local encoded=""
  local identifier_value
  local separator=""
  local -a values

  IFS='.' read -r -a values <<< "$value"
  for identifier_value in "${values[@]}"; do
    if [[ "$identifier_value" =~ ^[0-9]+$ ]]; then
      encoded+="${separator}n${identifier_value}a"
    else
      encoded+="${separator}s$(encode_bytes "$identifier_value")a"
    fi
    separator='b'
  done

  printf '%sa' "$encoded"
}

core_version="${without_build%%-*}"
deb_version="$core_version"
if [[ "$without_build" == *-* ]]; then
  deb_version="${deb_version}~$(encode_prerelease "${without_build#*-}")"
fi
if [[ "$version" == *+* ]]; then
  deb_version="${deb_version}+b$(encode_bytes "${version#*+}")"
fi

printf 'version=%s\n' "$version"
printf 'deb_version=%s\n' "$deb_version"
printf 'tag=%s\n' "$tag"
