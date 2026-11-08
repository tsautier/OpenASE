# Packaging

Everything here is generated or maintained by hand and **not published
automatically**. Nothing in this folder is submitted anywhere by CI.

- `windows/openase.iss`: Inno Setup script. The release workflow builds
  `OpenASE-<tag>-windows-x64-setup.exe` from it. Locally:
  `iscc /DAppVersion=0.9.1 packaging\windows\openase.iss` (after `cargo build --release`
  in `rust/`); the setup lands in `dist/`.
- `scoop/openase.json`: Scoop manifest (portable zip).
- `make-manifests.mjs`: writes the winget manifests.

Each release carries a `SHA256SUMS.txt` with the checksum of every package.

## winget

1. Publish the release, then download its `SHA256SUMS.txt` (the setup URL must be live).
2. Generate the manifests (Node, no dependencies):

   ```
   node packaging/make-manifests.mjs v0.9.1 SHA256SUMS.txt --date 2026-11-09
   ```

   They are written to `packaging/winget/0.9.1/` (package `TSautier.OpenASE`, schema 1.6.0,
   Inno installer, per-user scope). Only the setup is listed; the zip is left to Scoop.
3. Validate and submit, either way:
   - `winget validate packaging/winget/0.9.1`, then `winget install --manifest packaging/winget/0.9.1`
     in a sandbox, then open a PR to [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs)
     adding the files under `manifests/t/TSautier/OpenASE/0.9.1/`; or
   - [wingetcreate](https://github.com/microsoft/winget-create):
     first version `wingetcreate new <setup url>`, later versions
     `wingetcreate update TSautier.OpenASE --version 0.9.1 --urls <setup url> --submit`.

Winget runs the installer silently with Inno's standard switches. Unsigned installers can
trigger SmartScreen warnings in the pipeline's review; signing is described in
`.github/workflows/release.yml` (SignPath).

## Scoop

`scoop/openase.json` has `checkver` (GitHub releases) and `autoupdate` (URL templates and
hash read from `SHA256SUMS.txt`). Update its `version`, `url`, `extract_dir` and `hash`
for the new release (or run `checkver.ps1 -Update` from a bucket checkout), then either:

- open a PR to the [Extras bucket](https://github.com/ScoopInstaller/Extras) adding
  `bucket/openase.json` (they require a notable, stable app and a working `checkver`), or
- keep your own bucket: a repository with a `bucket/` folder containing this file, used with
  `scoop bucket add openase https://github.com/tsautier/scoop-openase` and
  `scoop install openase/openase`.

The `hash` in the committed manifest is a placeholder (zeros) until a release is chosen;
take the zip line from `SHA256SUMS.txt`.

## Settings

Settings live in `%APPDATA%\OpenASE` and are never touched by the installer, the uninstaller
or the package managers, which is why Scoop needs no `persist`.
