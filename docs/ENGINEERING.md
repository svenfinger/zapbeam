# Engineering notes

## Pipeline and codec defaults

`files.rs` inspects signatures without writing. React sends newly ingested canonical paths with unique job IDs to `optimizer.rs`. One native thread drains a bounded queue (10,000 retained jobs; 256 per request). React polls results every 150 ms while work is pending. Removal cancels jobs and discards their results; unique IDs prevent stale results attaching to re-added rows. No shell or filesystem plugin permissions are exposed.

OxiPNG is pinned to 10.2.1 with default features disabled: no Rayon, Zopfli, or CLI. Preset 2, `interlace=None`, `optimize_alpha=false`, 30-second cooperative timeout. Metadata stripping extends OxiPNG's Safe policy: keep ICC, sRGB, cICP, gamma, chromaticities, physical aspect/resolution, background, significant bits, sanitized EXIF, and HDR display chunks. Preserve the representation when background/significant-bit chunks depend on it. Remove text, timestamps, histograms and other nonessential ancillary chunks. Apple iDOT offsets are invalid after compression and are removed. APNG and signed caBX remain unsupported. ICC serialization is restored byte-for-byte. Exact normalized pixels validate palette/transparency changes.

MozJPEG 4.1.5 `jpegtran` uses default progressive/scan optimization in one pass with `-optimize -strict -maxscans 100 -copy all -maxmemory 131072 -outfile`. This keeps one encoder invocation per file rather than searching multiple candidates. MozJPEG may retain its sanitized input when recompression would grow it, so not every output is progressive. Its immutable temporary input has already been sanitized. Retain ICC, Adobe color transforms and JFIF density/aspect; remove comments and JFXX/JFIF previews. EXIF is rebuilt with orientation, resolution, color interpretation/calibration and dimensions, including selected Exif/Interoperability subdirectory tags. GPS, camera/device, capture dates, MakerNotes, thumbnails and padding are discarded. Both TIFF byte orders are supported. Namespace-aware XMP parsing permits removal only when every property is known to be descriptive (including ordinary macOS screenshot XMP). Rendering/HDR properties, unknown schemas, compressed PNG XMP, extended JPEG XMP and unknown JPEG APP payloads remain explicitly unsupported. MPF/HDR compound images and JUMBF/Content Credentials are rejected before any removal.

Candidate metadata must equal the source's expected rendering metadata, not the original private metadata. Candidate checks use raw metadata so unexpected retained private fields cannot pass by being filtered a second time. Both input and candidate are fully decoded; dimensions and exact pixels must match. These checks deliberately trade some speed for replacement confidence. Files with errors or no smaller candidate retain their original bytes, including metadata; optimization is not a guaranteed privacy scrub. Filesystem permissions, timestamps and portable xattrs retain their existing separate policy.

Inputs are limited to 64 MiB, 64 megapixels, and 16,384 pixels on either axis. Decoder allocation limit is 512 MiB; total process memory is higher because input, output, and pixel comparisons coexist. JPEG has a 30-second wall timeout and is killed/reaped on cancellation. OxiPNG cancellation is cooperative at pipeline boundaries: computation may finish after removal, but cannot commit after cancellation takes the gate.

## Replacement guarantees and limits

Regular writable files only. Direct symlinks and files with multiple hard links are rejected. Parent-directory symlinks resolve during ingestion. Portable extended attributes are copied and verified. On macOS, `com.apple.provenance` and `com.apple.macl` are owned by the OS and regenerated/merged for new inodes and app access grants: neither is copied, removed, nor compared. Filesystem ACLs remain separately checked; these exceptions do not grant permissions or disable OS checks. These filesystem bookkeeping/access-grant attributes are distinct from embedded C2PA Content Credentials; ACLs, resource forks, macOS filesystem flags, or ownership that cannot be preserved cause an error. Candidate UID and GID must match the source; a directory that gives new files a different group can therefore prevent replacement. Permissions, atime and mtime are copied. Birth time, inode identity and ctime change on replacement. Reading can update atime even when processing fails.

Candidates live beside the source. Full validation precedes the strictly-smaller test. Source identity, size, mtime and complete bytes are checked after compression. Access-related xattr updates can legitimately change ctime, so replacement refreshes and revalidates filesystem metadata, ownership, permissions and xattrs rather than treating ctime alone as an image edit. This refreshed snapshot must remain stable through preparation and the final check immediately before rename. Candidate contents are synced, then same-filesystem rename atomically replaces the directory entry. On Linux, directory sync is attempted after commit; a failure does not falsely report that the original is unchanged. On macOS it is omitted: opening a selected file’s parent directory for reading can block in privacy checks after a successful rename, because a file-picker grant does not grant directory enumeration. Candidate data is still synced before rename; directory-entry power-loss durability is not guaranteed.

This is not a universal crash-durability guarantee. There remains a narrow external-writer race between the final check and rename; Unix rename provides no compare-and-swap against unrelated writers. Avoid editing files concurrently. Network and unusual filesystems are untested. Cancellation cannot undo a rename that has already committed. RAII removes temporary files on handled success, error and cancellation; a forced process termination or power loss can leave temporary files, never a partially written candidate at the original name. Automatic crash-remnant deletion is deliberately absent to avoid deleting unrelated files.

## Sidecar development and distribution

Run `scripts/build-jpegtran.sh` with Rust, curl, shasum, CMake and a C compiler installed. The script verifies the v4.1.5 source archive SHA-256, uses a fixed build string and builds `jpegtran-static` with shared libraries, TurboJPEG, PNG input and SIMD disabled. This configuration avoids additional assembly toolchains. Compiler/SDK versions can still affect binary bytes: source/configuration are reproducible, not a claim of bit-identical builds across toolchains.

The script writes `src-tauri/binaries/jpegtran-<host-triple>`. Tauri `externalBin` bundles it alongside the application executable as `jpegtran`; Rust resolves only that sibling. No PATH fallback or user-installed codec is used. Build natively on each target. Linux builds must use a distribution appropriate for the desired glibc baseline. Ubuntu CI builds the sidecar before Rust checks. The manual workflow builds and inspects installers; see `RELEASE.md`. Linux GUI execution, installation, signing and notarization require separate verification.

## Licensing

Zapbeam code is MIT. OxiPNG is MIT; libdeflater and libdeflate-sys are Apache-2.0; vendored libdeflate is MIT. MozJPEG incorporates IJG, BSD-3-Clause and zlib terms, compatible with distribution in an MIT project while retaining separate notices. SIMD is disabled in this build; its upstream notice remains included. `docs/licenses/MOZJPEG.md` and `IJG.txt` retain upstream terms. Required acknowledgement: This software is based in part on the work of the Independent JPEG Group.

Metadata parsing uses `quick-xml` 0.42.0. Its MIT license and its `memchr` dependency’s MIT/Unlicense are compatible with Zapbeam’s MIT license; both notices are included in `docs/licenses/RUST.txt` and the bundle. XML parsing is local, bounded by the input limit and 32 levels; DTDs and entity references are rejected, with no external resolution.

The Rust codec/validation stack uses MIT, Apache-2.0, BSD-style and Zlib licenses (including image, png, zune, moxcms/pxfm and their compression helpers). Filesystem helpers tempfile, filetime, xattr and rustix have permissive licenses. `python3 scripts/codec-notices.py` regenerates `docs/licenses/RUST.txt`, which collects their upstream notice files, including vendored libdeflate. Notices are bundled as Tauri resources. Existing application/framework dependencies still need a complete release-wide transitive notice audit; the bundled codec notices cover the compression pipeline rather than every application dependency.

Zune crates omit license files in their published archives; `ZUNE-MIT.txt` retains the upstream MIT notice (Git blob `4d832b8fc1d8f8e39cbb14d064220ecc8c0b73e3`) for offline notice generation.

## Diagnostics

On macOS, diagnostics are written to `~/Library/Logs/io.github.svenfinger.zapbeam/compression.jsonl`; other platforms use Tauri's application log directory. Files use mode 0600. The log rotates at approximately 2 MiB and retains one previous file. Logging failure reports to stderr without stopping optimization.

Records contain build type, process ID, process-local job number, timestamps, stage durations, format, byte counts, category and error. Source paths, filenames and metadata values must not be logged. Ingestion and optimization use separate job numbers; correlate by order, time, format and size. `totalMs` includes queue delay; subtract the completed `queue_wait.stageMs` for processing duration. A stage start without a corresponding completion identifies a possible stall.

Result categories distinguish compression, no savings, unsupported input, filesystem failure, engine failure, input/output validation, source changes and cancellation. A no-savings result means a complete, validated candidate was not smaller. When investigating a failure, retain the app build identity, matching diagnostics, original hashes and a disposable copy with filesystem attributes. Share private samples only with explicit permission.

## Native verification

Build the sidecar first. Use fresh disposable copies, retaining pristine references outside the repository. Preserve permissions and portable xattrs when copying real samples; check the destination directory's group as well as file ownership. Never feed earlier optimized outputs into a fresh-input comparison.

The fixture exporter creates a new directory and refuses to overwrite an existing one:

```sh
ZAPBEAM_OUTPUT=/path/to/new-reference-corpus cargo test --locked --manifest-path src-tauri/Cargo.toml export_reliability_fixtures -- --ignored --nocapture
```

It generates JPEG/RGB PNG, optimized controls, an indexed PNG with background metadata, a synthetic signed-marker PNG, malformed data and a read-only control. The signed marker tests safe rejection, not cryptographic signature validity. For optional synthetic EXIF/ICC/privacy fixtures, install Pillow in a separate Python environment and run `python3 scripts/metadata-fixtures.py /path/to/new-metadata-corpus`. ICC creation timestamps can vary; retain a single generated reference set for comparisons.

Copy the reference corpus with attributes into a fresh directory (for example, `cp -pR /path/to/reference-corpus /path/to/new-disposable-corpus`). Run the native app, add the copies through the picker or drop target, and observe queued, processing, compressed, unchanged and error states. Check actual file sizes, savings and explanations. Exercise selection, removal, focus and scrolling; re-add optimized copies to check repeat behavior. Verify both development and packaged builds when changes affect IPC, startup or bundling. A browser preview cannot verify native IPC, picker grants or sidecar resolution.

Verify native outputs without modifying them:

```sh
ZAPBEAM_FIXTURES=/path/to/reference-corpus ZAPBEAM_OUTPUT=/path/to/processed-copies cargo test --locked --manifest-path src-tauri/Cargo.toml verify_desktop_corpus -- --ignored --nocapture
```

The verifier checks smaller-only changes, exact pixels, expected embedded metadata, modes, mtime, portable xattrs and absence of extra temporary files. Separately record hashes/inodes before processing to confirm unchanged and rejected entries were not replaced. Compare UI results as well: integrity alone does not establish that the interface reported the outcome correctly.

Two other ignored helpers accept contributor-supplied corpora. `reliability_corpus` takes `ZAPBEAM_FIXTURES`, processes temporary copies and asserts pixel/byte integrity and cleanup. `comparison_corpus` additionally takes a new `ZAPBEAM_OUTPUT` directory and writes per-file timing/size/status JSON beside it. Choose an output path whose adjacent JSON report does not already exist. These helpers bypass native picker, queue and IPC and do not reproduce every filesystem attribute of a native copy; they supplement native testing.

## Reproducible benchmark

```sh
cargo test --release --locked --manifest-path src-tauri/Cargo.toml benchmark -- --ignored --nocapture
```

The benchmark generates small and 1080p PNG/JPEG gradients in temporary directories. It requires no private images and reports per-file time, sizes, savings and batch time. Sum file durations for processing-only totals: batch time includes fixture creation. Finish builds before measuring, avoid competing codec work, and use fresh inputs for comparisons. Generated gradients are useful regression controls, not estimates of typical photographic savings.

## Verification limits

macOS verification does not establish Linux native behavior or packaging. Broad camera/ICC/HDR diversity, network volumes, fresh macOS permission states, disk-full conditions and forced termination require separate testing. Filesystem calls have no hard deadline and the PNG timeout is cooperative. Keep these limits distinct from the replacement guarantees above; passing a corpus is not a universal reliability claim.
