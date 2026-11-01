# Contributing to Zapbeam

Thanks for helping improve Zapbeam. This guide covers running the app locally, checking your changes, and submitting a contribution.

## Prerequisites

Use Node.js 24 LTS with npm (`.nvmrc`), stable Rust via [rustup](https://rustup.rs/), and Git. Install Rust tooling with:

```sh
rustup update stable
rustup component add rustfmt clippy
```

On **macOS**, install Xcode Command Line Tools with `xcode-select --install` (full Xcode also works), and install CMake.

On **Ubuntu 24.04**, install:

```sh
sudo apt-get update
sudo apt-get install -y cmake libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

A graphical desktop session and installed system fonts are needed to run the app. See the official [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for platform details.

## Development

```sh
npm ci
scripts/build-jpegtran.sh
npm run dev
```

The sidecar script downloads checksum-pinned MozJPEG source and builds the native JPEG executable. Run it before Rust checks, development, or desktop builds. End users receive the executable inside the application. `npm run dev` opens the desktop app.

For frontend work, `npm run dev:frontend` opens a browser preview at localhost:1420. Adding files requires the native desktop app.

## Validation and builds

| Command                                             | Purpose                                                  |
| --------------------------------------------------- | -------------------------------------------------------- |
| `npm test` / `npm run rust:test`                    | Frontend behavior and Rust inspection/optimization tests |
| `npm run check`                                     | Frontend lint, formatting, and strict TypeScript checks  |
| `npm run lint`                                      | Biome lint                                               |
| `npm run format:check` / `npm run format`           | Check / apply frontend formatting                        |
| `npm run typecheck`                                 | Check UI and Vite configuration types                    |
| `npm run rust:format:check` / `npm run rust:format` | Check / apply rustfmt                                    |
| `npm run rust:check`                                | Compile-check Rust with the lockfile                     |
| `npm run rust:lint`                                 | Clippy, treating warnings as errors                      |
| `npm run build`                                     | Type-check and build frontend into `dist/`               |
| `npm run build:desktop`                             | Build the native app and platform bundles                |

Native output is in `src-tauri/target/release/bundle/`. Build on the target OS with its system prerequisites.

## Submitting changes

Describe the problem, the change, and how you verified it. Run the relevant checks above, and report any platform checks you could not perform. For window or IPC changes, verify the native app rather than relying on a browser preview.

Review new dependencies for license compatibility and redistribution requirements. Keep both lockfiles updated and document changes to setup or behavior. Add an entry under Unreleased in [CHANGELOG.md](CHANGELOG.md) only for user-visible features, improvements, or fixes.
