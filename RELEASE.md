# Release builds

Zapbeam is preparing version 0.1.0. These builds are test artifacts, not a public release.

## Targets

| Platform            | GitHub-hosted runner | Installers              |
| ------------------- | -------------------- | ----------------------- |
| macOS Apple Silicon | `macos-15` (arm64)   | DMG                     |
| Linux x86_64        | `ubuntu-22.04`       | AppImage, Debian `.deb` |

macOS targets 11.0 or newer; the oldest supported OS still needs native testing. Ubuntu 22.04 provides a glibc 2.35 build baseline and WebKitGTK 4.1. Compatibility depends on the target system's desktop libraries as well as glibc; an AppImage is not universally portable. See [Tauri's Linux baseline guidance](https://v2.tauri.app/distribute/appimage/) and [GitHub's runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

## Build process

In GitHub Actions, select **Validate and build → Run workflow** for the desired branch. The workflow must exist on the default branch before GitHub exposes manual dispatch. Ordinary pushes and pull requests run Linux validation only. Manual runs validate both targets, build native installers, inspect their contents, and upload artifacts for 14 days. No tags or GitHub Releases are created; the workflow has only `contents: read` permission.

Build from a clean checkout with Node 24 (`.nvmrc`), the Rust toolchain in `rust-toolchain.toml`, Python 3.10+, CMake, a native C compiler, curl and shasum. macOS needs Xcode command-line tools. Linux needs the apt packages listed in the workflow. Build natively on each architecture.

```sh
npm ci
python3 scripts/verify-version.py
# macOS only: apply to both the codec and the app
export MACOSX_DEPLOYMENT_TARGET=11.0
scripts/build-jpegtran.sh
npm run check
npm test
npm run rust:format:check
npm run rust:check
npm run rust:lint
npm run rust:test
```

Then run the platform's build command (Tauri builds the frontend automatically):

```sh
# macOS arm64
npm run build:desktop -- --bundles dmg -- --locked
# Linux x86_64; extraction mode also works on CI without a FUSE mount
APPIMAGE_EXTRACT_AND_RUN=1 npm run build:desktop -- --bundles appimage,deb -- --locked
# Inspect the generated installers on the same host
python3 scripts/verify-packages.py
```

All version fields remain 0.1.0. npm and Cargo use committed lockfiles; Rust is pinned. MozJPEG 4.1.5 uses the existing verified archive checksum and fixed configuration. The codec is rebuilt once per job; npm downloads and Cargo builds are cached. Hosted runner images, Node 24 patch versions, system packages, SDKs and Tauri's downloaded packaging tools can change. This is a repeatable build procedure, not a bit-for-bit reproducibility guarantee. Retain the commit and job toolchain logs when comparing artifacts.

## Artifacts and checks

Outputs are under `src-tauri/target/release/bundle/`: `dmg/*.dmg`, `appimage/*.AppImage`, and `deb/*.deb`. The macOS app is staged under `macos/Zapbeam.app` and may be removed after DMG creation; use `--bundles app,dmg` to retain both outputs. GitHub artifact names start with `Zapbeam-0.1.0-` and include runner and architecture. Generated binaries and bundles are ignored by Git. Remove stale bundle outputs before inspecting a different build; the verifier rejects ambiguous installer sets.

Tauri consumes `src-tauri/binaries/jpegtran-<host-triple>` and places `jpegtran` beside `zapbeam`: `Zapbeam.app/Contents/MacOS/` on macOS and `usr/bin/` in Linux packages. The verifier mounts the DMG read-only or extracts both Linux installers, checks executable architectures and package identity/version, inspects `otool -L` or `ldd`, checks codec notices, and runs the bundled codec with an empty PATH. MozJPEG's JPEG library is statically linked; OS libraries remain dynamic. The application resolves this sibling executable directly and never searches for global compression utilities.

These checks establish package structure and codec execution. They do not establish desktop GUI behavior or successful installation, especially on Linux.

## Before public distribution

- Complete the application-wide transitive license/notice audit described in `docs/ENGINEERING.md`.
- Enroll in the Apple Developer Program and obtain a **Developer ID Application** identity. Store the exported certificate/private key and password in protected CI secrets, use a temporary keychain, and sign the app and nested sidecar with the appropriate hardened runtime settings. Tauri uses `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` and `APPLE_SIGNING_IDENTITY` for this setup.
- Configure Apple notarization authentication: an App Store Connect API key (`APPLE_API_ISSUER`, `APPLE_API_KEY`, `APPLE_API_KEY_PATH`) or Apple ID, app-specific password and team ID. Submit, wait for acceptance, staple and validate tickets. Verify Gatekeeper on a quarantined download from a clean Mac. Follow [Tauri's macOS signing guide](https://v2.tauri.app/distribute/sign/macos/).
- Separately create and securely back up a Tauri updater keypair. Keep `TAURI_SIGNING_PRIVATE_KEY` and its password in protected secrets; only the public key belongs in app configuration. Plan key recovery before shipping. Apple signing does not replace updater signatures.
- Implement and test the updater plugin, narrow permissions, update UI and HTTPS endpoints together. Generate the appropriate update payloads and signatures (macOS app archive and Linux AppImage, not a DMG or Debian package as the update payload). Publish a GitHub-hosted `latest.json` with version, notes, date, and platform entries `darwin-aarch64` and `linux-x86_64`, each containing a signature and immutable download URL. Follow [Tauri's updater guide](https://v2.tauri.app/plugin/updater/). Test upgrade and invalid-signature behavior before enabling it for users.
- Add a separately approved release/tag workflow with protected signing secrets, a narrowly scoped publishing job with `contents: write`, checksums and reviewed release notes. Publish manifests only once their signed assets are available. Decide retention and rollback procedures.
- Test installation, launch, picker/drop ingestion, JPEG/PNG optimization, cancellation/removal and uninstall on clean native systems without development tools. Use disposable images and the corpus procedure in `docs/ENGINEERING.md`. Test macOS quarantine/Gatekeeper and oldest supported OS; test Debian installation/dependencies and AppImage execution on Ubuntu 22.04 and newer Linux desktops, including Wayland/X11 and FUSE availability.

The current build workflow requires no external credentials. Do not add certificates, private keys, passwords or tokens to the repository. No updater configuration or signing key is provisioned by this build foundation.
