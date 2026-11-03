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
# macOS arm64: signed/notarized (credentials described below)
scripts/build-signed-macos.sh
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
- Separately create and securely back up a Tauri updater keypair. Keep `TAURI_SIGNING_PRIVATE_KEY` and its password in protected secrets; only the public key belongs in app configuration. Plan key recovery before shipping. Apple signing does not replace updater signatures.
- Implement and test the updater plugin, narrow permissions, update UI and HTTPS endpoints together. Generate the appropriate update payloads and signatures (macOS app archive and Linux AppImage, not a DMG or Debian package as the update payload). Publish a GitHub-hosted `latest.json` with version, notes, date, and platform entries `darwin-aarch64` and `linux-x86_64`, each containing a signature and immutable download URL. Follow [Tauri's updater guide](https://v2.tauri.app/plugin/updater/). Test upgrade and invalid-signature behavior before enabling it for users.
- Add a separately approved release/tag workflow with protected signing secrets, a narrowly scoped publishing job with `contents: write`, checksums and reviewed release notes. Publish manifests only once their signed assets are available. Decide retention and rollback procedures.
- Test installation, launch, picker/drop ingestion, JPEG/PNG optimization, cancellation/removal and uninstall on clean native systems without development tools. Use disposable images and the corpus procedure in `docs/ENGINEERING.md`. Test macOS quarantine/Gatekeeper and oldest supported OS; test Debian installation/dependencies and AppImage execution on Ubuntu 22.04 and newer Linux desktops, including Wayland/X11 and FUSE availability.

Push/PR validation and Linux builds require no Apple credentials. Never commit certificates, private keys, passwords, temporary keychains or credential files. Apple signing does not configure the updater.

## macOS signing and notarization

The manual `macos-sign` job uses the GitHub Environment named `release`. The existing six secrets are environment secrets; the previous workflow did not reference that environment or any Apple secrets. Repository secrets with the same names also work (environment values take precedence). Only the macOS signing step receives them.

| Secret                       | Value                                                                     |
| ---------------------------- | ------------------------------------------------------------------------- |
| `APPLE_CERTIFICATE`          | Base64-encoded Developer ID Application `.p12`, including its private key |
| `APPLE_CERTIFICATE_PASSWORD` | Export password for that `.p12`                                           |
| `APPLE_SIGNING_IDENTITY`     | Full `Developer ID Application: Name (TEAMID)` identity                   |
| `APPLE_API_ISSUER`           | App Store Connect Team API key issuer UUID                                |
| `APPLE_API_KEY`              | App Store Connect key ID                                                  |
| `APPLE_API_PRIVATE_KEY`      | Complete `.p8` contents, including PEM header/footer and newlines         |

The team expectation is the ten-character Team ID in the configured signing identity, not the API issuer UUID. Configure a Team API key with Developer access. The certificate must be valid and include its private key. No additional keychain password secret is needed.

In repository **Settings → Environments → release**, consider required reviewers and a deployment branch restriction to trusted branches. At inspection this environment had neither restriction. These settings must be configured in GitHub; the workflow does not change them. Review workflow and build-script changes before granting a branch access to signing secrets. Repository-secret installations can retain their secrets while adding these environment protections.

`scripts/build-signed-macos.sh` fails on missing credentials or an invalid identity format. It writes the `.p8` under the runner temporary directory with mode 600 inside a mode 700 directory, exports `APPLE_API_KEY_PATH`, and removes the directory on exit or handled termination. Tauri imports the certificate into its temporary keychain and deletes that keychain and decoded certificate through its own cleanup. Hosted runners are disposable, including after forced cancellation. Do not enable shell tracing or debug logging around credentials.

Following [Tauri's signing guide](https://v2.tauri.app/distribute/sign/macos/), Tauri signs the MozJPEG sidecar and application inside out with hardened runtime, submits the app to Apple, waits for acceptance, and staples the app. It then creates and signs the DMG containing that stapled app. No bundle files are changed afterward. The DMG is not separately submitted or stapled: its enclosed app carries the validated ticket. This behavior was checked against the locked Tauri CLI 2.12.4 bundler implementation; recheck it when upgrading Tauri.

The signing step runs `verify-packages.py --require-signing` before artifact upload. It checks the mounted app's resource seal, exact Developer ID authority, Team ID, bundle identifier, timestamp, hardened runtime, every bundled Mach-O signature, the DMG signature, Gatekeeper assessment and the app's stapled ticket. Any failure fails the job. The structural-only verifier remains available for local unsigned development builds and Linux.

### Manual signed build

On macOS arm64, complete the prerequisites and checks above. Load the six environment variables securely from your credential manager without putting values in shell history. Move any previous `src-tauri/target/release/bundle` directory outside the repository first; the signing script refuses stale outputs. Then run:

```sh
scripts/build-signed-macos.sh
```

For local development without credentials, `npm run build:desktop -- --bundles dmg -- --locked` still builds an unsigned test artifact. It is not suitable for distribution and is expected to fail release signature verification.

To repeat release verification, only the expected identity is needed in the environment:

```sh
python3 scripts/verify-packages.py --require-signing
```

For an independently downloaded DMG, mount it read-only and inspect the actual enclosed app (replace the example DMG path):

```sh
hdiutil attach /path/to/Zapbeam_0.1.0_aarch64.dmg -readonly -nobrowse -mountpoint /tmp/zapbeam-check
codesign --verify --strict --verbose=2 /path/to/Zapbeam_0.1.0_aarch64.dmg
codesign -dv --verbose=4 /path/to/Zapbeam_0.1.0_aarch64.dmg
codesign --verify --deep --strict --verbose=2 /tmp/zapbeam-check/Zapbeam.app
codesign -dv --verbose=4 /tmp/zapbeam-check/Zapbeam.app
codesign --verify --strict --verbose=2 /tmp/zapbeam-check/Zapbeam.app/Contents/MacOS/zapbeam
codesign --verify --strict --verbose=2 /tmp/zapbeam-check/Zapbeam.app/Contents/MacOS/jpegtran
codesign -dv --verbose=4 /tmp/zapbeam-check/Zapbeam.app/Contents/MacOS/jpegtran
spctl --assess --type execute --verbose=4 /tmp/zapbeam-check/Zapbeam.app
xcrun stapler validate /tmp/zapbeam-check/Zapbeam.app
hdiutil detach /tmp/zapbeam-check
```

Check the displayed authority and Team ID against the configured identity and the app identifier against `io.github.svenfinger.zapbeam`. A valid signature alone does not prove notarization; a ticket alone does not replace signature verification.

### Troubleshooting and downloaded-install test

The DMG from Actions run `41311122892` was inspected read-only. Its app reproduces `code has no resources but signature indicates they must be present`. `codesign -dv` reports `adhoc,linker-signed`, `Info.plist=not bound`, `TeamIdentifier=not set`, and `Sealed Resources=none`; `Contents/_CodeSignature/CodeResources` is absent although Info.plist, Resources and MacOS are present. The pre-packaging local app reproduces the same condition. The confirmed issue is a missing bundle-level signature/resource seal: the executable's linker signature is not a signed app bundle. There is no evidence that quarantine removal repaired it or that notarization alone could fix it.

- Missing credentials: check the `release` environment, secret names and branch/reviewer policies. GitHub does not reveal secret values through its API.
- Certificate import/identity failure: check that the `.p12` includes the private key, the export password is correct, and the full identity matches the certificate. Do not substitute `-` (ad-hoc signing).
- Notarization rejection: inspect Tauri's submission result and Apple's notary log using the submission ID and the same API key authentication with `xcrun notarytool log`. Check certificate validity, nested code and hardened runtime. Keep credentials and private logs outside the repository.
- Signature/resource-seal failure: inspect the app inside the DMG, compare with the staging app if retained, and rebuild from clean bundle output. Do not modify or re-sign the packaged app as a workaround.
- Ticket or Gatekeeper failure: preserve the failed artifact for diagnosis, check Apple service/network availability, and rerun the signed build. Do not suppress verification failures or upload that build as valid.

After pushing these changes, manually dispatch the workflow. Download the successful macOS artifact through a browser on a clean Mac, extract the artifact ZIP, open the DMG and drag Zapbeam into Applications. Confirm the browser download has quarantine metadata (`xattr -l`), then launch normally from Finder without removing quarantine, bypassing Gatekeeper, or locally re-signing. Verify the installed app with the signature, Gatekeeper and stapler commands above. Test picker/drop ingestion and optimization with disposable JPEG/PNG files. Repeat offline after installation to exercise the stapled ticket. Record the macOS version and result; CI verification is not a substitute for this installation test.
