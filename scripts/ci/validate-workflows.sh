#!/usr/bin/env bash
set -euo pipefail

workflow_dir=".github/workflows"
expected=("ci.yml" "release.yml")

mapfile -t actual < <(
  find "$workflow_dir" -maxdepth 1 -type f \
    \( -name "*.yml" -o -name "*.yaml" \) \
    -printf "%f\n" | sort
)

if [[ "${actual[*]}" != "${expected[*]}" ]]; then
  echo "Active workflows must be exactly: ${expected[*]}" >&2
  echo "Found: ${actual[*]:-(none)}" >&2
  exit 1
fi

for workflow in "${actual[@]}"; do
  if ! grep -Eq '^name: GTTY ' "$workflow_dir/$workflow"; then
    echo "$workflow must have a GTTY-owned workflow name." >&2
    exit 1
  fi
done

forbidden=(
  "namespace-profile-ghostty"
  "ghostty-org/ghostty"
  "CACHIX_AUTH_TOKEN"
  "SENTRY_AUTH_TOKEN"
  "SNAPCRAFT_STORE_CREDENTIALS"
  "FLATPAK"
  "VOUCH_APP_"
)

for token in "${forbidden[@]}"; do
  if grep -R --line-number --fixed-strings "$token" "$workflow_dir"; then
    echo "Forbidden upstream CI/CD dependency found: $token" >&2
    exit 1
  fi
done

if grep -R --line-number -E 'runs-on:.*(self-hosted|namespace-profile)' "$workflow_dir"; then
  echo "GTTY workflows must use GitHub-hosted runners." >&2
  exit 1
fi

echo "GTTY workflow policy is valid."
