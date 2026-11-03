# Working on Zapbeam

Zapbeam is a local-first desktop media optimizer for macOS and Linux. JPEG/PNG files added through native selection or drops are automatically optimized losslessly and retained in a session-only list.

## Structure and responsibilities

- `src/`: React presentation, interaction state, and explicit Tauri invocations.
- `src-tauri/src/`: Rust commands and desktop startup. Read-only file inspection lives in `files.rs`; the bounded worker, codecs, validation, and replacement live in `optimizer.rs`.
- `src-tauri/tauri.conf.json` and `capabilities/`: window, bundle, CSP, and security configuration.
- `.github/workflows/validate.yml`: Ubuntu validation and manual macOS/Linux installer builds.
- `RELEASE.md`: build commands, artifact locations and distribution prerequisites. Keep Apple credentials confined to the manual macOS signing job. Publishing requires an explicit request.
- `rust-toolchain.toml` pins Rust; keep version fields in npm, Cargo and Tauri aligned and run `scripts/verify-version.py` when changing them.
- Keep command contracts narrow and explicit. Validate untrusted inputs in Rust; TypeScript types are not a security boundary.

## Engineering expectations

- Prefer small, direct implementations. Add modules when behavior needs them; avoid empty service layers or speculative architecture.
- Keep TypeScript strict. Use Biome for JS/TS/JSON/CSS and rustfmt/Clippy for Rust.
- Make loading, failure, and completion states understandable. Avoid unhandled promise rejections and production panics for expected failures.
- Startup speed and responsiveness matter. CPU-heavy compression and filesystem work must not block the UI or async runtime; keep background work bounded and preserve cancellation guarantees.
- Operate offline by default. Do not add telemetry, uploads, remote assets, or accounts without an explicit product requirement.
- Keep Tauri permissions and CSP narrow. No unrestricted filesystem or shell access. Treat paths, file contents, and engine arguments as untrusted; avoid logging private paths or content unnecessarily.
- React must not directly own filesystem or compression logic.

## Verification and dependencies

- Run `npm run check`, `npm run build`, `npm run rust:format:check`, `npm run rust:check`, and `npm run rust:lint` for relevant changes.
- For IPC, window, or startup changes, launch `npm run dev` and verify the native window and backend status. A browser preview cannot verify native IPC.
- Add focused tests for validation and processing changes, covering failures and edge cases. Do not add tests that merely repeat static markup.
- Attempt `npm run build:desktop` for packaging changes. Report exactly what could not be verified; do not imply macOS verification proves Linux support.
- Use npm and Cargo; retain both lockfiles. Prefer established dependencies and explain additions. Review licenses, redistribution requirements, and MIT compatibility separately, including bundled binaries. Add compression engines only as part of an explicitly requested optimization implementation.

## Documentation audience

- `README.md`, `CHANGELOG.md`, and `CONTRIBUTING.md` are public documentation. Write for users and outside contributors, never as an internal progress report or agent handoff. Include information only when it helps that audience understand, use, or contribute to Zapbeam.
- Keep `README.md` focused on Zapbeam’s purpose and user-facing workflow. Do not add internal progress tracking, implementation details, internal plans, or validation reports.
- Avoid internal planning jargon such as “milestone,” “phase,” “sprint,” or numbered delivery stages in public-facing content. Describe the product and user-visible changes in plain language.
- Describe capabilities positively. Do not clutter public documentation with absent features, “not available yet,” “not implemented,” “does not do X,” or similar caveats. Keep implementation gaps and development status in internal documentation.
- We are working toward the initial 0.1.0 release. Until that release, collect user-visible changes under `Unreleased`; commits are not releases. Do not invent preview releases, add release dates or version sections, or present in-progress work as released, ready, or verified functionality. Keep the README a project introduction rather than a running account of development progress.
- Keep `CHANGELOG.md` limited to user-visible features, improvements, and fixes. Describe their effect on users. Do not add entries for internal refactors, tooling, documentation edits, engineering decisions, or test results.
- Keep `CONTRIBUTING.md` focused on actionable guidance for outside contributors: prerequisites, setup, running and building the app, relevant checks, and submitting changes. Do not use it for architecture notes, implementation inventories, future engineering requirements, dependency audit records, or detailed internal test scripts.
- Put agent instructions and engineering conventions in `AGENTS.md`; put longer internal design notes, dependency reviews, and manual verification procedures in `docs/` (currently `docs/ENGINEERING.md`). Do not duplicate them in the public guides.
- Keep investigation reports, private-image manifests, local diagnostics and disposable corpora outside the repository. Preserve reusable procedures and current limitations in `docs/ENGINEERING.md`.
- Before editing public documentation, check every addition against its audience. A code change does not automatically require updates to all three public files. Keep Markdown filenames uppercase, using the conventional `CONTRIBUTING.md` name for the contribution guide. Update these instructions when engineering conventions change.

## File ingestion

- Rust inspects signatures (up to eight bytes) and metadata without decoding or changing source files. Canonical paths identify duplicates, including symlinks; distinct hard links are separate entries.
- `useFiles.ts` serializes ingestion in batches of at most 256, owns native listener cleanup, and supports removing selected entries without filesystem changes. `files.ts` contains pure queue and formatting helpers.
- Permit only native dialog opening and event listen/unlisten. Do not enable filesystem or shell plugin commands.
- Run `npm test` and `npm run rust:test` for ingestion changes.

## Interface and optimization

- Keep the interface to a full-window drop target or compact selectable table and a floating Add Files button. React owns selection; `selection.ts` contains its pure transitions. No separate Optimize action, footer, or persistent success bar.
- Unprocessed files use a neutral empty circle and unknown savings. A spinner means genuine work only; never simulate compression or claim inspection is optimization.
- Optimization starts immediately on adding files, preserves input format, and overwrites originals only for smaller validated outputs. Keep inspection read-only.
- Process into a temporary file on the same filesystem; validate the complete output and require a smaller valid image before replacing the original. Design crash-safe replacement (including durability and cleanup), preserve relevant metadata and permissions where possible, and detect source changes before replacement. Failures or non-smaller results leave the original unchanged.
- Results show final size and percentage saved relative to original size. Keep results until explicitly removed or the app closes. Removing entries must never delete files from disk.
- Run `npm test` for selection changes; manually verify range/toggle selection, arrows, select-all, deletion, focus after removal, and scrolling.

- Build the pinned native JPEG sidecar with `scripts/build-jpegtran.sh` before Rust checks or desktop builds. See `docs/ENGINEERING.md` for configuration, safety limits, benchmarks, and licensing.
- Keep one compression worker and OxiPNG parallelism disabled. Cancellation and replacement share a commit gate; never remove it without replacing its guarantee. Preserve portable xattrs (leave macOS-managed com.apple.provenance/com.apple.macl to the OS), reject ACLs/hard links/flags, and verify exact pixels and the expected rendering-critical metadata before replacement. Remove embedded private/nonessential metadata by default; keep EXIF orientation and color interpretation. Reject signed, compound and unrecognized rendering metadata before stripping.
- Keep compression diagnostics local, bounded and free of source paths, filenames and metadata values. Preserve typed failure categories and underlying OS/engine causes. Use the disposable-corpus workflow in `docs/ENGINEERING.md` when investigating native-app failures.
