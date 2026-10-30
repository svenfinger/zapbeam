# Engineering notes

Internal implementation context and manual verification procedures. See [AGENTS.md](../AGENTS.md) for engineering requirements.

## Upcoming compression behavior

Adding images will immediately schedule optimization, with no separate Optimize action. Preserve input format by default and replace originals only after producing and validating a smaller result. Failed processing or a result that is not smaller must leave the original unchanged. Show final size and percentage saved relative to original size; retain results until the user removes entries or closes the application. Removing an entry never deletes its source file.

Rust must write to a temporary file on the same filesystem, fully validate the result, and safely replace the original only after verification. Design for crash-safe replacement, durability, source-change detection, temporary-file cleanup, and preservation of relevant metadata and permissions where possible. Apply these requirements when implementing compression.

The UI currently shows neutral unprocessed status and unknown savings. The status component supports processing, compressed, unchanged, and error presentation for future real results; none are fabricated. The inspection spinner represents only actual metadata inspection. Errors appear only when needed and can be dismissed; success announcements are screen-reader-only.

## Stack and boundaries

Tauri 2, React, TypeScript, Vite, Rust, and npm. React owns presentation and queue state; Rust owns filesystem inspection through `inspect_files`. The read-only `backend_status` command remains a startup handshake. Files are inspected in sequential batches of up to 256 using blocking tasks; only eight signature bytes are read. Signature recognition does not prove that the full image is valid. Canonical paths deduplicate symlinks, but not distinct hard links. No source writes or image decoding occur.

Only dialog-open and event-listen/unlisten permissions are enabled. No filesystem or shell commands are exposed. Production content uses a restrictive CSP; development permits the local Vite connection.

### Dependency review

The native picker uses the official `@tauri-apps/plugin-dialog` / `tauri-plugin-dialog` (MIT OR Apache-2.0). Its new Rust dependencies include `tauri-plugin`, `tauri-plugin-fs` (MIT OR Apache-2.0) and `rfd` (MIT); `serde` (MIT OR Apache-2.0) provides typed serialization. These licenses permit use with the MIT project; retain applicable license/copyright notices in redistribution. The filesystem dependency is internal to the dialog plugin and grants no frontend filesystem permissions. No compression engines or separately distributed executable binaries are added. Platform packaging still needs a full transitive license/notice review before release.

## Manual verification

The native window should show the image drop area. Add a PNG and JPEG, add them again to check duplicates, test click/Shift-click/Cmd-or-Ctrl-click selection, arrow and Shift-arrow navigation, and Delete/Backspace removal. Cmd/Ctrl+A followed by Delete clears the list. Test a mixed drop with an unsupported file and a directory; check that valid files remain. Cancel the picker and verify Cmd/Ctrl+O. Close the window and stop the terminal process with Ctrl+C when finished.

`npm run dev:frontend` starts only Vite at localhost:1420. In a regular browser, the UI reports that file ingestion needs the desktop window because Tauri is absent. For a failure-path smoke check, open that URL in a browser and confirm the error is visible.
