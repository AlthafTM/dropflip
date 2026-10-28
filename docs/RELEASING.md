# Windows releases

The [Windows release workflow](../.github/workflows/release.yml) builds a portable ZIP and a per-user Inno Setup installer from the same native executable. It includes the project license, Dictately notice and resolved Rust dependency license texts. Conversion engine binaries are excluded from both packages; their setup and licensing remain separate.

## Make a release

1. Update the package version in `Cargo.toml`, run `cargo check` to refresh the root entry in `Cargo.lock`, and update the numeric/string versions in `assets/dropflip.rc`. Keep all three consistent. Versions use `X.Y.Z` or `X.Y.Z-rc.N`.
2. Run the Rust and applicable conversion checks in [qa/README.md](../qa/README.md). Review dependency and CodeQL results. Commit and push to `main`.
3. Run **Actions → Windows release → Run workflow** against `main`. This builds and uploads test packages without creating a release. Verify the installer and portable app from those artifacts.
4. Tag the tested commit and push the tag:

   ```powershell
   git tag -a v0.1.0 -m "Dropflip 0.1.0"
   git push origin v0.1.0
   ```

The workflow rejects a tag/version mismatch, builds with locked dependencies and a static Windows C runtime, checks formatting/tests and dependency advisories, collects dependency notices, tests portable/installed conversions and uninstall, creates SHA-256 checksums and GitHub provenance attestations, then publishes the release. An `-rc.N` tag creates a prerelease. Stable tags update the latest release link. The publisher refuses to overwrite an already published release. Interrupted draft releases can resume; assets are only replaced while the release remains a draft.

Build jobs have read-only repository access. Only the tag publication job receives release-write and attestation permissions. Actions are pinned to commit hashes. Inno Setup 6.7.3 is downloaded from its upstream immutable release and checked against its pinned SHA-256 before execution. Installer builds use a fixed, per-user application directory and require no elevation. Uninstall removes installed files/shortcuts; it does not recursively delete the user's app settings or converted files.

The current packages are **unsigned**. The workflow does not bypass SmartScreen or fabricate a signing identity. Build attestations and checksums are not Windows Authenticode signatures. Adding code signing requires a maintainer-controlled signing certificate/service and a separate protected signing step.

## Build packages locally

From a Windows Rust developer shell:

```powershell
cargo test --locked
cargo build --release --locked
.\scripts\setup-installer.ps1
.\scripts\package-release.ps1
```

Artifacts appear in `dist/<version>/`. The packager verifies the binary's product version and fails if a resolved dependency has no license text. Where a crate omits its workspace license, the packager obtains notices from that crate's recorded Git commit on GitHub, records the origin and caches the text. It refuses an existing bundle directory; use a new `-OutputRoot` when rebuilding. `-BinaryPath`, `-CompilerPath`, `-CargoCommand` and `-Target` support local toolchains. The release workflow uses `x86_64-pc-windows-msvc`.

Run `scripts/test-package.ps1 -ReleaseDirectory dist/<version>` to check both packages in an isolated temporary directory. It disables shortcut/startup tasks, checks the installed binary matches the ZIP, validates converted content, uninstalls, and verifies that an unrelated user file and the existing startup shortcut remain intact. The check refuses a Windows account where Dropflip is already installed; use a clean account or VM in that case. CI runs this check before upload/publication.
