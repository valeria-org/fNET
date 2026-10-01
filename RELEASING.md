# Releasing fNET

Target repository: <https://github.com/valeria-org/fNET>.
Crate name: `fNET`; initial version: `0.1.0`; release tag: `v0.1.0`.
The library is MIT licensed. Preserve LICENSE, NOTICE, and upstream attribution.

## Prepare and verify

Python 3.11+, stable Rust with rustfmt/clippy, and Git are needed for release
preparation. Normal crate users need only Rust 1.85+; Python/.NET are not build
dependencies. Install Rust 1.85.0 as well to run the minimum-version CI checks.

```powershell
python tools/release.py check --tag v0.1.0 --allow-dirty
python tools/release.py artifacts --tag v0.1.0 --allow-dirty
```

The first command validates metadata, license, version/tag consistency, the
changelog, generated catalogue, feature tests, formatting, clippy, docs, and a
crates.io publish dry run. The second builds the same checked `.crate` plus a
source archive, version-specific release notes, and SHA256SUMS in `target/dist/`.
It includes uncommitted source when `--allow-dirty` is supplied; release workflows
require a clean tagged commit. Neither command pushes nor uploads anything.
Add `--offline` to use Cargo's offline package verification instead of the
registry publish dry run when preparing without network access.

For subsequent releases, update Cargo.toml and Cargo.lock versions and add a
matching CHANGELOG.md section before running these commands with the new tag.

## First repository setup

Create `valeria-org/fNET` on GitHub if it does not exist, then commit the prepared
project and push its `main` branch. The local origin URL is configured for that
repository. The following commands perform the public release setup:

```powershell
git add .
git commit -m "Prepare fNET 0.1.0 release"
git push -u origin main
git tag -a v0.1.0 -m "fNET 0.1.0"
git push origin v0.1.0
```

Pushing a version tag runs `.github/workflows/release.yml`: it validates the
tagged commit, checks Rust 1.85 and Windows support, and creates a **draft GitHub
release** with the crate, source archive, checksums, and release notes. The tag
must match the package version. Workflow reruns update the existing draft;
they refuse to change an already published GitHub release.

## Publish to crates.io

Log in to crates.io, verify the publishing account's email address, and create a
token authorized to publish `fNET`. Store it as the repository Actions secret
`CARGO_REGISTRY_TOKEN`. Account setup is documented in the
[Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html).

The `Publish to crates.io` workflow is **manual**. Dispatch it after reviewing
the draft and checksums, supplying the tag of the intended release:

```powershell
gh workflow run publish.yml --repo valeria-org/fNET -f tag=v0.1.0
```

The workflow checks out that exact tag, validates it, and publishes `fNET` with
the stored token. It is independent of draft creation; pushing a tag does not
publish the crate. To publish locally instead:

```powershell
python tools/release.py check --tag v0.1.0 --require-tag
cargo login
cargo publish --locked --registry crates-io
```

After confirming the crate and docs.rs documentation are available, publish the
GitHub draft:

```powershell
gh release edit v0.1.0 --repo valeria-org/fNET --draft=false
```

Crates.io ownership starts with the publishing account. To share ownership with
the organization, add an existing GitHub team using `cargo owner --add
github:valeria-org:TEAM`; substitute the actual team slug. See
[Cargo's team ownership documentation](https://doc.rust-lang.org/cargo/reference/publishing.html#github-permissions).
