# Working on Zapbeam

Zapbeam is a local-first desktop media optimizer for macOS and Linux. This repository currently contains only the Milestone 01 development shell; optimization and file workflows are planned.

## Structure and responsibilities

- `src/`: React presentation, interaction state, and explicit Tauri invocations.
- `src-tauri/src/`: Rust commands and desktop startup. Future file validation, filesystem operations, orchestration, and compression belong here.
- `src-tauri/tauri.conf.json` and `capabilities/`: window, bundle, CSP, and security configuration.
- `.github/workflows/validate.yml`: Ubuntu static validation.
- Keep command contracts narrow and explicit. Validate untrusted inputs in Rust; TypeScript types are not a security boundary.

## Engineering expectations

- Prefer small, direct implementations. Add modules when behavior needs them; avoid empty service layers or speculative architecture.
- Keep TypeScript strict. Use Biome for JS/TS/JSON/CSS and rustfmt/Clippy for Rust.
- Make loading, failure, and completion states understandable. Avoid unhandled promise rejections and production panics for expected failures.
- Startup speed and responsiveness matter. Future CPU-heavy compression and filesystem work must not block the UI or async runtime; use bounded background work and design cancellation when processing is introduced.
- Operate offline by default. Do not add telemetry, uploads, remote assets, or accounts without an explicit product requirement.
- Keep Tauri permissions and CSP narrow. No unrestricted filesystem or shell access. Treat future paths, file contents, and engine arguments as untrusted; avoid logging private paths or content unnecessarily.
- React must not directly own filesystem or compression logic.

## Verification and dependencies

- Run `npm run check`, `npm run build`, `npm run rust:format:check`, `npm run rust:check`, and `npm run rust:lint` for relevant changes.
- For IPC, window, or startup changes, launch `npm run dev` and verify the native window and backend status. A browser preview cannot verify native IPC.
- Add focused tests as real validation and processing behavior arrives, covering failures and edge cases. Do not add tests that merely repeat static markup.
- Attempt `npm run build:desktop` for packaging changes. Report exactly what could not be verified; do not imply macOS verification proves Linux support.
- Use npm and Cargo; retain both lockfiles. Prefer established dependencies and explain additions. Review licenses, redistribution requirements, and MIT compatibility separately, including bundled binaries. Do not add compression engines until their milestone.
- Keep the README focused on a public introduction. Maintain setup, prerequisites, development scope, and validation commands in contribution.md; keep CHANGELOG.md public-facing and focused on user-visible features, improvements, and fixes, rather than internal tooling or documentation changes. Update these instructions when engineering conventions change.
