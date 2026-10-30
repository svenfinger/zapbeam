# Contributing to Zapbeam

Thanks for helping build Zapbeam. Keep changes focused, preserve local-first operation, and favor a small dependency footprint.

## Development scope

The application currently contains a React development shell that confirms communication with Rust. File handling and optimization are not implemented. JPEG and PNG are the initial focus; additional image formats and other media can be evaluated separately. Compression engines have not been selected.

## Stack and boundaries

Tauri 2, React, TypeScript, Vite, Rust, and npm. React owns presentation and interactions; Rust will own filesystem access, validation, processing orchestration, and compression engines. Explicit Tauri commands connect them. The current read-only `backend_status` command returns `"ready"`.

No filesystem, shell, or opener plugin permissions are enabled. Production content uses a restrictive CSP; development permits the local Vite connection.

## Prerequisites

Use Node.js 24 LTS with npm (`.nvmrc`), stable Rust via [rustup](https://rustup.rs/), and Git. Install Rust tooling with:

```sh
rustup update stable
rustup component add rustfmt clippy
```

On **macOS**, install Xcode Command Line Tools with `xcode-select --install` (full Xcode also works).

On **Ubuntu 24.04**, install:

```sh
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

A graphical desktop session and installed system fonts are needed to run the app. See the official [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for platform details.

## Development

```sh
npm ci
npm run dev
```

The native window should show **Rust backend connected.** Close the window and stop the terminal process with Ctrl+C when finished.

`npm run dev:frontend` starts only Vite at localhost:1420. In a regular browser, the shell deliberately reports a backend connection error because Tauri is absent. For a failure-path smoke check, open that URL in a browser and confirm the error is visible.

## Validation and builds

| Command                                             | Purpose                                                 |
| --------------------------------------------------- | ------------------------------------------------------- |
| `npm run check`                                     | Frontend lint, formatting, and strict TypeScript checks |
| `npm run lint`                                      | Biome lint                                              |
| `npm run format:check` / `npm run format`           | Check / apply frontend formatting                       |
| `npm run typecheck`                                 | Check UI and Vite configuration types                   |
| `npm run rust:format:check` / `npm run rust:format` | Check / apply rustfmt                                   |
| `npm run rust:check`                                | Compile-check Rust with the lockfile                    |
| `npm run rust:lint`                                 | Clippy, treating warnings as errors                     |
| `npm run build`                                     | Type-check and build frontend into `dist/`              |
| `npm run build:desktop`                             | Build the native app and platform bundles               |

Native output is in `src-tauri/target/release/bundle/`. Build on the target OS with its system prerequisites. Local builds do not configure release signing, notarization, or publishing.

GitHub Actions runs static validation on Ubuntu for pull requests and pushes to main. Native launch and IPC still require a desktop smoke check; Linux runtime behavior must be checked on Linux. There is no automated UI test suite yet.

Both `package-lock.json` and `src-tauri/Cargo.lock` belong in version control. Update dependencies intentionally and rerun validation.

## Submitting changes

Describe the problem, the change, and how you verified it. Run the relevant checks above, and report any platform checks you could not perform. For window or IPC changes, verify the native app rather than relying on a browser preview.

Review new dependencies for license compatibility and redistribution requirements. Keep both lockfiles updated, document changes to setup or behavior, and add a concise entry under Unreleased in [CHANGELOG.md](CHANGELOG.md).

See [AGENTS.md](AGENTS.md) for repository structure and engineering conventions.
