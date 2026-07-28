# GTTY CI/CD

GTTY owns its GitHub Actions configuration. Ghostty upstream workflows are not
executed on the `release` branch and are not copied into GTTY feature branches.

## Branch model

- `main` mirrors Ghostty upstream and is not a GTTY development branch.
- `release` is the GTTY integration and publication branch.
- Every GTTY change starts from `release` and returns through a pull request
  targeting `yuebanddd/ghostty:release`.
- GTTY changes are never proposed to Ghostty upstream unless the repository
  owner makes a separate, explicit decision.

## Active workflows

Only these files may exist under `.github/workflows` on `release`:

- `ci.yml` — policy, Rust-if-present, Linux, and macOS checks.
- `release.yml` — unsigned macOS/Linux preview packages and GitHub prereleases.

`scripts/ci/validate-workflows.sh` enforces this allowlist and blocks known
Ghostty runner names, repository gates, and private release secrets.

## CI triggers

GTTY CI runs on:

- pull requests targeting `release`;
- pushes to `release`;
- explicit manual dispatch.

The Linux job uses `ubuntu-24.04` with Nix and no Cachix account. The
macOS job uses the GitHub-hosted `macos-26` arm64 runner. Rust checks activate
automatically after a root Cargo workspace is added.

## Release triggers and artifacts

`release.yml` runs only when:

- a `gtty-v*` tag is pushed; or
- a maintainer starts a manual dispatch.

A manual dispatch is a dry run: it builds downloadable workflow artifacts but
does not create a GitHub Release. A valid semantic tag such as `gtty-v0.1.0`
builds:

- Linux x86_64 and aarch64 tarballs;
- macOS arm64 and x86_64 zip archives;
- `SHA256SUMS`.

Until signing is configured, filenames contain `unsigned` and GitHub Releases
are marked as prereleases. No Apple Developer ID, notarization, Linux package
signature, Snap, Flatpak, Homebrew, Sentry, Cachix account, or upstream release
credential is used.

## Upstream synchronization

Never merge `main` directly into `release`.

1. Update `main` from Ghostty upstream without adding GTTY commits.
2. Create a dedicated synchronization branch from `release`.
3. Merge the updated `main` into that branch.
4. Restore `.github/workflows` to the two GTTY-owned files and remove every
   upstream workflow reintroduced by the merge.
5. Run `bash scripts/ci/validate-workflows.sh`.
6. Build and test on both platforms, then open a pull request to `release`.

The workflow policy job is the final guard: an upstream sync cannot pass CI
while an additional executable workflow remains under `.github/workflows`.

## Future signing configuration

Signing and notarization belong to a later release milestone. When added:

- secrets must be GTTY-owned and documented by name and purpose;
- fork pull requests must never receive signing secrets;
- unsigned dry-run artifacts must remain available;
- release publication must stay restricted to `gtty-v*` tags;
- signed and unsigned assets must be unmistakably named.
