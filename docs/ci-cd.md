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

The Linux CI job uses `ubuntu-24.04` with Nix and no Cachix account. The
macOS CI job uses the GitHub-hosted `macos-26` arm64 runner. Locked Rust
formatting, lint, unit, and integration checks run for the GTTY workspace.

## Release triggers and artifacts

`release.yml` runs when:

- any pull request targets `release`;
- a `gtty-v*` tag is pushed; or
- a maintainer starts a manual dispatch.

Every pull request builds and verifies all four installers using an internal
`0.0.0-pr.<number>` version. Running the installer gate for every PR prevents
new build inputs from bypassing packaging checks as the upstream source tree
evolves. It uploads short-lived workflow artifacts but cannot create a GitHub
Release.

A manual dispatch from `release` is a dry run: it builds the same verified
installers as a tagged release and exposes them as downloadable workflow
artifacts, but does not create a GitHub Release. A valid semantic tag such as
`gtty-v0.1.0` must point to a commit contained in `release` and builds:

- Ubuntu 24.04-compatible Debian packages for amd64 and arm64;
- macOS disk images for Apple Silicon and Intel;
- `SHA256SUMS`.

The Debian package installs the terminal as `ghostty`, adds `gtty` as the GTTY
command, and installs the `gttyd` local daemon. Because it owns the same
terminal resources, it declares that it conflicts with and replaces an
installed `ghostty` Debian package. It also carries its own
`gtk4-layer-shell` runtime under `/usr/lib/gtty`; Ubuntu 24.04 does not provide
that GTK4 library, so the terminal uses a package-relative RPATH and requires no
third-party apt repository.

Linux packaging first generates Ghostty's official-format source tarball. That
tarball contains the precompiled Blueprint GTK resources required by downstream
packagers; compiling a raw Git checkout would require a newer
`blueprint-compiler` than Ubuntu 24.04 provides.

The macOS disk image contains `GTTY.app`, an `Applications` shortcut, and embeds
`gttyd` in the application bundle. Packaging assigns the main app and embedded
bundles GTTY-owned identifiers under `com.yuebanddd.gtty`, and stamps the
semantic release's numeric version into the app metadata before signing.

Until Developer ID signing and notarization are configured, macOS filenames
contain `unsigned`, the app uses an ad-hoc signature, and GitHub Releases are
marked as prereleases. Linux packages are also unsigned. No Apple Developer ID,
Linux package signature, Snap, Flatpak, Homebrew, Sentry, Cachix account, or
upstream release credential is used.

## Creating a release

1. Merge a fully reviewed change into `release`.
2. Run `GTTY Release` manually on `release` with the intended semantic version.
3. Download and install the workflow artifacts on the target machines if an
   additional hands-on check is needed.
4. Create and push `gtty-v<version>` at the same `release` commit.
5. Wait for all four installers to pass their installation checks.
6. Download the `.dmg` or `.deb` from GitHub Releases and verify it against
   `SHA256SUMS`.

The tagged workflow refuses a tag that is not contained in `release`, refuses
malformed semantic versions, and will not publish a partial asset set.

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
