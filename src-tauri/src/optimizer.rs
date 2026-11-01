//! One worker, bounded input sizes, and a cancellation gate shared with final replacement.
use serde::{Deserialize, Serialize};
#[cfg(target_os = "macos")]
use std::os::darwin::fs::MetadataExt as DarwinMetadataExt;
use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::{Cursor, Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};
use tempfile::NamedTempFile;

use crate::diagnostics::{Category, Failure, Result, Trace, failure};
#[path = "optimizer_metadata.rs"]
mod metadata_policy;
const MAX_BYTES: u64 = 64 * 1024 * 1024;
#[derive(Clone, Deserialize)]
pub struct Request {
    pub id: String,
    pub path: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    id: String,
    state: &'static str,
    category: Option<Category>,
    size: Option<u64>,
    savings: Option<f64>,
    error: Option<String>,
}
struct Job {
    request: Request,
    cancelled: Mutex<bool>,
    trace: Mutex<Trace>,
}
#[derive(Default)]
struct Queue {
    pending: VecDeque<Arc<Job>>,
    jobs: HashMap<String, Arc<Job>>,
    results: HashMap<String, Update>,
}
#[derive(Clone)]
pub struct Optimizer(Arc<(Mutex<Queue>, Condvar)>);
impl Optimizer {
    pub fn new() -> Self {
        let service = Self(Arc::new((Mutex::new(Queue::default()), Condvar::new())));
        let worker = service.clone();
        std::thread::spawn(move || worker.work());
        service
    }
    fn work(&self) {
        loop {
            let job = {
                let (lock, ready) = &*self.0;
                let mut q = lock.lock().unwrap();
                while q.pending.is_empty() {
                    q = ready.wait(q).unwrap();
                }
                let job = q.pending.pop_front().unwrap();
                if *job.cancelled.lock().unwrap() {
                    continue;
                }
                q.results.insert(
                    job.request.id.clone(),
                    Update {
                        id: job.request.id.clone(),
                        state: "processing",
                        category: None,
                        size: None,
                        savings: None,
                        error: None,
                    },
                );
                job
            };
            let result = {
                let mut trace = job.trace.lock().unwrap();
                let result = optimize_traced(
                    Path::new(&job.request.path),
                    &sidecar(),
                    &job.cancelled,
                    2,
                    || {},
                    &mut trace,
                );
                let success = match &result {
                    Ok((a, b)) if b < a => Category::Compressed,
                    _ => Category::NoSavings,
                };
                trace.finish(result, success)
            };
            let mut q = self.0.0.lock().unwrap();
            if q.jobs.contains_key(&job.request.id) {
                let mut update = Update {
                    id: job.request.id.clone(),
                    state: "error",
                    category: None,
                    size: None,
                    savings: None,
                    error: None,
                };
                match result {
                    Ok((original, size)) => {
                        update.state = if size < original {
                            "compressed"
                        } else {
                            "unchanged"
                        };
                        update.category = Some(if size < original {
                            Category::Compressed
                        } else {
                            Category::NoSavings
                        });
                        update.size = Some(size);
                        if size < original {
                            update.savings =
                                Some(100.0 * (original - size) as f64 / original as f64);
                        }
                    }
                    Err(e) => {
                        update.category = e.downcast_ref::<Failure>().map(|e| e.category);
                        update.error = Some(
                            e.downcast_ref::<Failure>()
                                .map_or_else(|| e.to_string(), Failure::explanation),
                        );
                    }
                }
                q.results.insert(job.request.id.clone(), update);
            }
        }
    }
}
fn sidecar() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("jpegtran")
}
#[tauri::command]
pub async fn schedule_optimization(
    requests: Vec<Request>,
    service: tauri::State<'_, Optimizer>,
) -> std::result::Result<(), &'static str> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut q = service.0.0.lock().map_err(|_| "Queue unavailable")?;
        enqueue(&mut q, requests)?;
        service.0.1.notify_one();
        Ok(())
    })
    .await
    .map_err(|_| "Could not schedule optimization")?
}
fn enqueue(q: &mut Queue, requests: Vec<Request>) -> std::result::Result<(), &'static str> {
    if requests.len() > 256 || q.jobs.len() + requests.len() > 10000 {
        return Err("Too many queued files");
    }
    for request in &requests {
        if Path::new(&request.path).canonicalize().ok().as_deref() != Some(Path::new(&request.path))
        {
            return Err("Use a canonical file path");
        }
        if request.id.len() > 100 || !Path::new(&request.path).is_absolute() {
            return Err("Invalid request");
        }
    }
    for request in requests {
        if q.jobs.contains_key(&request.id)
            || q.jobs.values().any(|j| j.request.path == request.path)
        {
            continue;
        }
        let mut trace = Trace::new();
        trace.stage("queue_wait", Category::Queued);
        let job = Arc::new(Job {
            request,
            cancelled: Mutex::new(false),
            trace: Mutex::new(trace),
        });
        q.jobs.insert(job.request.id.clone(), job.clone());
        q.pending.push_back(job);
    }
    Ok(())
}
#[tauri::command]
pub async fn cancel_optimization(
    ids: Vec<String>,
    service: tauri::State<'_, Optimizer>,
) -> std::result::Result<(), String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut q = service.0.0.lock().unwrap();
        cancel_jobs(&mut q, ids);
    })
    .await
    .map_err(|_| "Could not cancel processing".into())
}
fn cancel_jobs(q: &mut Queue, ids: Vec<String>) {
    for id in ids {
        if let Some(job) = q.jobs.remove(&id) {
            *job.cancelled.lock().unwrap() = true;
        }
        q.results.remove(&id);
    }
    q.pending.retain(|j| !*j.cancelled.lock().unwrap());
}
#[tauri::command]
pub async fn optimization_results(
    service: tauri::State<'_, Optimizer>,
) -> std::result::Result<Vec<Update>, &'static str> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        service
            .0
            .0
            .lock()
            .unwrap()
            .results
            .values()
            .cloned()
            .collect()
    })
    .await
    .map_err(|_| "Could not read optimization results")
}

fn check_cancel(cancel: &Mutex<bool>) -> Result<()> {
    if *cancel.lock().unwrap() {
        Err(failure(Category::Cancelled, "Processing cancelled"))
    } else {
        Ok(())
    }
}
fn read(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(failure(Category::Unsupported, "Image exceeds 64 MiB limit"));
    }
    Ok(bytes)
}
fn decode(bytes: &[u8]) -> Result<image::DynamicImage> {
    let (width, height) = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .into_dimensions()?;
    if u64::from(width) * u64::from(height) > 64_000_000 {
        return Err(failure(
            Category::Unsupported,
            "Image exceeds 64 megapixel limit",
        ));
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Jpeg | image::ImageFormat::Png)
    ) {
        return Err(failure(Category::Unsupported, "Unsupported image format"));
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode()?;
    if u64::from(image.width()) * u64::from(image.height()) > 64_000_000 {
        return Err(failure(
            Category::Unsupported,
            "Image exceeds 64 megapixel limit",
        ));
    }
    Ok(image)
}
fn same_pixels(a: &image::DynamicImage, b: &image::DynamicImage) -> bool {
    if a.width() != b.width() || a.height() != b.height() {
        return false;
    }
    if a.color() == b.color() {
        return a.as_bytes() == b.as_bytes();
    }
    // PNG may reduce color type or bit depth losslessly. Normalize one pixel at
    // a time, preserving all 16 bits, instead of allocating two RGBA16 images.
    match (normalized_pixels(a), normalized_pixels(b)) {
        (Some(a), Some(b)) => a.eq(b),
        _ => false,
    }
}
fn normalized_pixels(
    image: &image::DynamicImage,
) -> Option<Box<dyn Iterator<Item = [u16; 4]> + '_>> {
    use image::{DynamicImage::*, Pixel};
    macro_rules! eight {
        ($buffer:expr) => {
            Box::new(
                $buffer
                    .pixels()
                    .map(|p| p.to_rgba().0.map(|v| u16::from(v) * 257)),
            )
        };
    }
    macro_rules! sixteen {
        ($buffer:expr) => {
            Box::new($buffer.pixels().map(|p| p.to_rgba().0))
        };
    }
    Some(match image {
        ImageLuma8(b) => eight!(b),
        ImageLumaA8(b) => eight!(b),
        ImageRgb8(b) => eight!(b),
        ImageRgba8(b) => eight!(b),
        ImageLuma16(b) => sixteen!(b),
        ImageLumaA16(b) => sixteen!(b),
        ImageRgb16(b) => sixteen!(b),
        ImageRgba16(b) => sixteen!(b),
        // PNG/JPEG decoders never return floating-point pixels.
        _ => return None,
    })
}
// Preserve ancillary PNG chunks byte-for-byte; reject APNG and trailing data.
fn png_metadata(bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut offset = 8;
    let mut chunks = Vec::new();
    while offset + 12 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
        let end = offset.checked_add(len + 12).ok_or("Invalid PNG chunk")?;
        if end > bytes.len() {
            return Err("Truncated PNG".into());
        }
        let name = &bytes[offset + 4..offset + 8];
        if name == b"caBX" {
            return Err(failure(
                Category::Unsupported,
                "Contains signed Content Credentials; optimization would invalidate them. Original retained.",
            ));
        }
        if name == b"acTL" {
            return Err(failure(
                Category::Unsupported,
                "Animated PNG is not supported",
            ));
        }
        // iDOT is Apple's compressed-data offset index. Recompression invalidates
        // it, so OxiPNG removes it even with StripChunks::None. Never restore
        // stale offsets or confuse their removal with lost image metadata.
        if name[0].is_ascii_lowercase() && name != b"tRNS" && name != b"iDOT" {
            chunks.push(bytes[offset..end].to_vec());
        }
        offset = end;
        if name == b"IEND" {
            if offset != bytes.len() {
                return Err(failure(Category::Unsupported, "PNG has trailing data"));
            }
            return Ok(chunks);
        }
    }
    Err("PNG is missing its end marker".into())
}
fn jpeg_metadata(bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut pos = 2;
    let mut markers = Vec::new();
    while pos < bytes.len() {
        if bytes[pos] != 255 {
            pos += 1;
            continue;
        }
        pos += 1;
        while pos < bytes.len() && bytes[pos] == 255 {
            pos += 1;
        }
        let marker = *bytes.get(pos).ok_or("Truncated JPEG")?;
        pos += 1;
        if marker == 0xd9 {
            if pos != bytes.len() {
                return Err(failure(Category::Unsupported, "JPEG has trailing data"));
            }
            return Ok(markers);
        }
        if marker == 0 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let len = u16::from_be_bytes(
            bytes
                .get(pos..pos + 2)
                .ok_or("Truncated JPEG marker")?
                .try_into()?,
        ) as usize;
        if len < 2 || pos + len > bytes.len() {
            return Err("Invalid JPEG marker".into());
        }
        if marker == 0xe2 && bytes[pos + 2..pos + len].starts_with(b"MPF\0") {
            return Err(failure(
                Category::Unsupported,
                "Multi-picture/HDR JPEG is not supported; original and HDR data retained.",
            ));
        }
        if marker == 0xeb && bytes[pos + 2..pos + len].starts_with(b"JP") {
            return Err(failure(
                Category::Unsupported,
                "JPEG contains JUMBF/Content Credentials; original retained to preserve signed metadata.",
            ));
        }
        if (0xe0..=0xef).contains(&marker) || marker == 0xfe {
            let mut data = vec![marker];
            data.extend_from_slice(&bytes[pos..pos + len]);
            markers.push(data);
        }
        pos += len;
    }
    Err("JPEG is missing its end marker".into())
}
fn validate_replaceable(meta: &fs::Metadata) -> Result<()> {
    if !meta.is_file() || meta.nlink() != 1 || meta.permissions().readonly() {
        return Err("Requires a writable regular file with one hard link".into());
    }
    if meta.len() > MAX_BYTES {
        return Err(failure(Category::Unsupported, "Image exceeds 64 MiB limit"));
    }
    #[cfg(target_os = "macos")]
    if meta.st_flags() != 0 {
        return Err("File has filesystem flags; original retained".into());
    }
    Ok(())
}
fn same_content_metadata(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    a.dev() == b.dev()
        && a.ino() == b.ino()
        && a.len() == b.len()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
}
fn same(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    same_content_metadata(a, b) && a.ctime() == b.ctime() && a.ctime_nsec() == b.ctime_nsec()
}
#[cfg(test)]
fn optimize(path: &Path, jpegtran: &Path, cancel: &Mutex<bool>, preset: u8) -> Result<(u64, u64)> {
    optimize_inner(path, jpegtran, cancel, preset, || {})
}
#[cfg(test)]
fn optimize_inner(
    path: &Path,
    jpegtran: &Path,
    cancel: &Mutex<bool>,
    preset: u8,
    before_commit: impl FnOnce(),
) -> Result<(u64, u64)> {
    let mut trace = Trace::new();
    let result = optimize_traced(path, jpegtran, cancel, preset, before_commit, &mut trace);
    let success = match &result {
        Ok((a, b)) if b < a => Category::Compressed,
        _ => Category::NoSavings,
    };
    trace.finish(result, success)
}
fn optimize_traced(
    path: &Path,
    jpegtran: &Path,
    cancel: &Mutex<bool>,
    preset: u8,
    before_commit: impl FnOnce(),
    trace: &mut Trace,
) -> Result<(u64, u64)> {
    trace.stage("source_stat", Category::Filesystem);
    check_cancel(cancel)?;
    let meta = fs::symlink_metadata(path)?;
    trace.source_size = Some(meta.len());
    trace.stage("source_permissions", Category::Filesystem);
    validate_replaceable(&meta)?;
    // Check early so unsupported filesystem metadata does not waste codec work.
    trace.stage("source_xattrs", Category::Filesystem);
    attributes(path)?;
    trace.stage("source_acl", Category::Filesystem);
    check_acl(path)?;
    trace.stage("source_read", Category::Filesystem);
    let source = read(path)?;
    let png = source.starts_with(b"\x89PNG\r\n\x1a\n");
    trace.format = Some(if png { "PNG" } else { "JPEG" });
    trace.stage("source_metadata", Category::InputValidation);
    let metadata = if png {
        metadata_policy::png_expected(&source)?
    } else {
        metadata_policy::jpeg_expected(&source)?
    };
    trace.stage("source_decode", Category::InputValidation);
    let pixels = decode(&source)?;
    let parent = path.parent().ok_or("Missing parent directory")?;
    trace.stage("candidate_create", Category::Filesystem);
    let mut candidate = tempfile::Builder::new()
        .prefix(".zapbeam-")
        .suffix(if png { ".png" } else { ".jpg" })
        .tempfile_in(parent)?;
    if png {
        trace.stage("oxipng", Category::Engine);
        let output = optimize_png(&source, preset)?;
        trace.stage("candidate_write", Category::Filesystem);
        candidate.write_all(&output)?;
    } else {
        trace.stage("jpegtran", Category::Engine);
        optimize_jpeg(&source, parent, candidate.path(), jpegtran, cancel)?;
    }
    check_cancel(cancel)?;
    trace.stage("candidate_read", Category::Filesystem);
    let output = read(candidate.path())?;
    trace.candidate_size = Some(output.len() as u64);
    trace.stage("candidate_metadata", Category::OutputValidation);
    if png != output.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("Output format changed".into());
    }
    let after_metadata = if png {
        png_metadata(&output)?
    } else {
        jpeg_metadata(&output)?
    };
    if metadata != after_metadata || (png && source.get(28) != output.get(28)) {
        return Err("Output metadata changed; original retained".into());
    }
    trace.stage("candidate_pixels", Category::OutputValidation);
    let after = decode(&output)?;
    if pixels.width() != after.width()
        || pixels.height() != after.height()
        || !same_pixels(&pixels, &after)
    {
        return Err("Output pixels changed; original retained".into());
    }
    before_commit();
    let gate = cancel.lock().unwrap();
    if *gate {
        return Err(failure(Category::Cancelled, "Processing cancelled"));
    }
    // Opening a file in macOS can update access-related xattrs and ctime.
    // Content identity uses inode, size, mtime and exact bytes instead. Refresh
    // metadata for replacement, then require that refreshed snapshot to stay stable.
    trace.stage("source_recheck", Category::Filesystem);
    let current = fs::symlink_metadata(path)?;
    if !same_content_metadata(&meta, &current) || source != read(path)? {
        return Err(failure(
            Category::SourceChanged,
            "Image contents changed during processing; original retained",
        ));
    }
    trace.stage("savings", Category::OutputValidation);
    if output.len() >= source.len() {
        return Ok((meta.len(), meta.len()));
    }
    trace.stage("replacement_permissions", Category::Filesystem);
    validate_replaceable(&current)?;
    trace.stage("source_acl", Category::Filesystem);
    check_acl(path)?;
    trace.stage("replacement_xattrs", Category::Filesystem);
    let attributes = self::attributes(path)?;
    let meta = current;
    if !same(&meta, &fs::symlink_metadata(path)?) {
        return Err(failure(
            Category::SourceChanged,
            "File metadata is still changing; original retained",
        ));
    }
    trace.stage("candidate_ownership", Category::Filesystem);
    let target_meta = candidate.as_file().metadata()?;
    if target_meta.uid() != meta.uid() || target_meta.gid() != meta.gid() {
        return Err("Cannot preserve file ownership; original retained".into());
    }
    trace.stage("candidate_xattrs", Category::Filesystem);
    for (name, value) in &attributes {
        xattr::set(candidate.path(), name, value)?;
    }
    let copied_attributes = self::attributes(candidate.path())?;
    if attributes != copied_attributes {
        return Err("Could not preserve extended attributes; original retained".into());
    }
    trace.stage("candidate_acl", Category::Filesystem);
    check_acl(candidate.path())?;
    trace.stage("candidate_permissions", Category::Filesystem);
    fs::set_permissions(candidate.path(), meta.permissions())?;
    trace.stage("candidate_timestamps", Category::Filesystem);
    filetime::set_file_times(
        candidate.path(),
        filetime::FileTime::from_last_access_time(&meta),
        filetime::FileTime::from_last_modification_time(&meta),
    )?;
    trace.stage("candidate_sync", Category::Filesystem);
    candidate.as_file().sync_all()?;
    trace.stage("precommit_check", Category::Filesystem);
    if !same(&meta, &fs::symlink_metadata(path)?) || attributes != self::attributes(path)? {
        return Err(failure(
            Category::SourceChanged,
            "Source changed before replacement; original retained",
        ));
    }
    trace.stage("atomic_replace", Category::Filesystem);
    candidate.persist(path).map_err(|e| e.error)?;
    // Rename is the commit point. macOS file-picker grants cover the selected
    // file, not directory enumeration: opening Desktop/Downloads for directory
    // fsync can block in OS privacy checks after the replacement already succeeded.
    // Keep candidate fsync above, but do not request extra folder access on macOS.
    #[cfg(not(target_os = "macos"))]
    let _ = fs::File::open(parent).and_then(|f| f.sync_all());
    Ok((meta.len(), output.len() as u64))
}

// macOS ACLs are separate from listxattr. Linux POSIX ACLs appear as xattrs.
#[cfg(target_os = "macos")]
fn check_acl(path: &Path) -> Result<()> {
    use std::{
        ffi::{CString, c_char, c_int, c_void},
        os::unix::ffi::OsStrExt,
    };
    unsafe extern "C" {
        fn acl_get_file(path: *const c_char, kind: c_int) -> *mut c_void;
        fn acl_get_entry(acl: *mut c_void, entry: c_int, result: *mut *mut c_void) -> c_int;
        fn acl_free(acl: *mut c_void) -> c_int;
    }
    let path = CString::new(path.as_os_str().as_bytes())?;
    // ACL_TYPE_EXTENDED and ACL_FIRST_ENTRY from macOS sys/acl.h.
    unsafe {
        let acl = acl_get_file(path.as_ptr(), 0x100);
        if acl.is_null() && std::io::Error::last_os_error().raw_os_error() == Some(2) {
            return Ok(());
        }
        if acl.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut entry = std::ptr::null_mut();
        let result = acl_get_entry(acl, 0, &mut entry);
        acl_free(acl);
        if result == 0 {
            return Err("File has an ACL; original retained".into());
        }
    }
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn check_acl(_: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{
        ImageEncoder,
        codecs::{
            jpeg::JpegEncoder,
            png::{CompressionType, FilterType, PngEncoder},
        },
    };
    use std::os::unix::fs::PermissionsExt;
    pub(super) fn fixture(path: &Path, png: bool, width: u32, height: u32) {
        let data: Vec<u8> = (0..width * height)
            .flat_map(|i| [(i % 251) as u8, (i / width % 251) as u8, 120])
            .collect();
        let mut file = fs::File::create(path).unwrap();
        if png {
            PngEncoder::new_with_quality(&mut file, CompressionType::Fast, FilterType::NoFilter)
                .write_image(&data, width, height, image::ExtendedColorType::Rgb8)
                .unwrap();
        } else {
            JpegEncoder::new_with_quality(&mut file, 90)
                .encode(&data, width, height, image::ExtendedColorType::Rgb8)
                .unwrap();
        }
    }
    pub(super) fn engine() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "binaries/jpegtran-{}",
            env!("TAURI_ENV_TARGET_TRIPLE")
        ))
    }
    #[test]
    fn real_engines_preserve_pixels_and_replace_only_smaller() {
        for png in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("image");
            fixture(&path, png, 257, 131);
            let original = fs::read(&path).unwrap();
            let meta = fs::metadata(&path).unwrap();
            let (before, after) = optimize(&path, &engine(), &Mutex::new(false), 2).unwrap();
            assert!(after < before);
            assert_eq!(before, original.len() as u64);
            assert_eq!(
                decode(&original).unwrap().to_rgba16(),
                decode(&fs::read(&path).unwrap()).unwrap().to_rgba16()
            );
            assert_eq!(fs::metadata(&path).unwrap().mtime(), meta.mtime());
            let stable = fs::read(&path).unwrap();
            let result = optimize(&path, &engine(), &Mutex::new(false), 2).unwrap();
            assert_eq!(result.0, result.1);
            assert_eq!(fs::read(&path).unwrap(), stable);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
    #[test]
    fn jpeg_default_optimizes_sequential_and_progressive_inputs() {
        // Read frame markers only in the header, never in entropy-coded data.
        fn frame_marker(bytes: &[u8]) -> u8 {
            let mut offset = 2;
            loop {
                assert_eq!(bytes[offset], 0xff);
                let marker = bytes[offset + 1];
                if marker == 0xc0 || marker == 0xc2 {
                    return marker;
                }
                assert_ne!(marker, 0xda, "missing frame header before scans");
                let length = u16::from_be_bytes([bytes[offset + 2], bytes[offset + 3]]);
                offset += 2 + usize::from(length);
            }
        }
        for progressive in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("source.jpg");
            fixture(&path, false, 640, 480);
            if progressive {
                let converted = dir.path().join("progressive.jpg");
                assert!(
                    Command::new(engine())
                        .args([
                            "-revert",
                            "-progressive",
                            "-optimize",
                            "-copy",
                            "all",
                            "-outfile"
                        ])
                        .arg(&converted)
                        .arg(&path)
                        .status()
                        .unwrap()
                        .success()
                );
                fs::rename(converted, &path).unwrap();
            }
            let original = fs::read(&path).unwrap();
            assert_eq!(
                frame_marker(&original),
                if progressive { 0xc2 } else { 0xc0 }
            );
            let (before, after) = optimize(&path, &engine(), &Mutex::new(false), 2).unwrap();
            assert!(after < before);
            let output = fs::read(&path).unwrap();
            assert_eq!(frame_marker(&output), 0xc2);
            assert!(same_pixels(
                &decode(&original).unwrap(),
                &decode(&output).unwrap()
            ));
            assert_eq!(
                metadata_policy::jpeg_expected(&original).unwrap(),
                jpeg_metadata(&output).unwrap()
            );
            // An already optimized progressive source must remain byte-identical.
            let (before, after) = optimize(&path, &engine(), &Mutex::new(false), 2).unwrap();
            assert_eq!(before, after);
            assert_eq!(fs::read(&path).unwrap(), output);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn errors_and_cancellation_leave_sources_and_no_temps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image");
        for data in [b"hello".as_slice(), b"\x89PNG\r\n\x1a\n", b"\xff\xd8\xff"] {
            fs::write(&path, data).unwrap();
            assert!(optimize(&path, &engine(), &Mutex::new(false), 2).is_err());
            assert_eq!(fs::read(&path).unwrap(), data);
        }
        fixture(&path, false, 31, 17);
        let original = fs::read(&path).unwrap();
        assert!(optimize(&path, Path::new("/missing-jpegtran"), &Mutex::new(false), 2).is_err());
        assert!(optimize(&path, Path::new("/usr/bin/false"), &Mutex::new(false), 2).is_err());
        assert!(optimize(&path, &engine(), &Mutex::new(true), 2).is_err());
        let cancel = Mutex::new(false);
        assert!(
            optimize_inner(&path, &engine(), &cancel, 2, || *cancel.lock().unwrap() =
                true)
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        assert!(optimize(&path, &engine(), &Mutex::new(false), 2).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let changed = b"external edit";
        assert!(
            optimize_inner(&path, &engine(), &Mutex::new(false), 2, || fs::write(
                &path, changed
            )
            .unwrap())
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), changed);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn metadata_updates_do_not_look_like_image_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        fixture(&path, true, 257, 131);
        let (original, final_size) =
            optimize_inner(&path, &engine(), &Mutex::new(false), 2, || {
                xattr::set(&path, "user.zapbeam-access", b"fresh metadata").unwrap();
            })
            .unwrap();
        assert!(final_size < original);
        assert_eq!(
            xattr::get(&path, "user.zapbeam-access").unwrap().unwrap(),
            b"fresh metadata"
        );
        let stable = fs::read(&path).unwrap();
        let sizes = optimize_inner(&path, &engine(), &Mutex::new(false), 2, || {
            xattr::set(&path, "user.zapbeam-access", b"accessed again").unwrap();
        })
        .unwrap();
        assert_eq!(sizes.0, sizes.1);
        assert_eq!(fs::read(&path).unwrap(), stable);
    }
    #[test]
    fn permissions_changed_during_processing_are_respected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        fixture(&path, true, 257, 131);
        let original = fs::read(&path).unwrap();
        assert!(
            optimize_inner(&path, &engine(), &Mutex::new(false), 2, || {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
            })
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), original);
    }
    #[test]
    fn exact_pixel_comparison_does_not_discard_low_bits() {
        let gray = image::DynamicImage::ImageLuma8(image::ImageBuffer::from_pixel(
            3,
            7,
            image::Luma([17]),
        ));
        let rgba = image::DynamicImage::ImageRgba16(image::ImageBuffer::from_pixel(
            3,
            7,
            image::Rgba([4369, 4369, 4369, 65535]),
        ));
        assert!(same_pixels(&gray, &rgba));
        let changed = image::DynamicImage::ImageRgba16(image::ImageBuffer::from_pixel(
            3,
            7,
            image::Rgba([4370, 4369, 4369, 65535]),
        ));
        assert!(!same_pixels(&gray, &changed));
        assert!(!same_pixels(&rgba, &changed));
    }
    #[test]
    fn compound_jpeg_is_rejected_before_any_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.jpg");
        fixture(&path, false, 31, 17);
        let jpeg = fs::read(&path).unwrap();
        for (marker, payload, reason) in [
            (0xe2, b"MPF\0".as_slice(), "HDR"),
            (0xeb, b"JP\0\x01".as_slice(), "Content Credentials"),
        ] {
            let mut input = jpeg[..2].to_vec();
            input.extend([255, marker]);
            input.extend(((payload.len() + 2) as u16).to_be_bytes());
            input.extend(payload);
            input.extend(&jpeg[2..]);
            fs::write(&path, &input).unwrap();
            assert!(
                optimize(&path, &engine(), &Mutex::new(false), 2)
                    .unwrap_err()
                    .to_string()
                    .contains(reason)
            );
            assert_eq!(fs::read(&path).unwrap(), input);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn os_provenance_is_not_treated_as_portable_file_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        fixture(&path, true, 257, 131);
        xattr::set(&path, "user.keep", b"important metadata").unwrap();
        assert!(
            attributes(&path)
                .unwrap()
                .iter()
                .all(|(name, _)| name != "com.apple.provenance" && name != "com.apple.macl")
        );
        let (a, b) = optimize(&path, &engine(), &Mutex::new(false), 2).unwrap();
        assert!(b < a);
        assert_eq!(
            xattr::get(&path, "user.keep").unwrap().unwrap(),
            b"important metadata"
        );
    }
    #[test]
    fn links_and_extended_attributes_are_retained() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image");
        fixture(&path, true, 31, 17);
        let link = dir.path().join("link");
        fs::hard_link(&path, &link).unwrap();
        assert!(optimize(&path, &engine(), &Mutex::new(false), 2).is_err());
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(optimize(&link, &engine(), &Mutex::new(false), 2).is_err());
        xattr::set(&path, "user.zapbeam", b"preserve").unwrap();
        optimize(&path, &engine(), &Mutex::new(false), 2).unwrap();
        assert_eq!(
            xattr::get(&path, "user.zapbeam").unwrap().unwrap(),
            b"preserve"
        );
    }
    #[test]
    #[ignore = "reproducible release benchmark; prints CSV"]
    fn benchmark() {
        println!("preset,format,width,height,ms,original,final,saved_percent");
        for preset in [1, 2] {
            let batch = Instant::now();
            for (png, width, height) in [
                (true, 257, 131),
                (true, 1920, 1080),
                (false, 257, 131),
                (false, 1920, 1080),
            ] {
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("image");
                fixture(&path, png, width, height);
                let start = Instant::now();
                let (a, b) = optimize(&path, &engine(), &Mutex::new(false), preset).unwrap();
                println!(
                    "{preset},{},{width},{height},{:.2},{a},{b},{:.2}",
                    if png { "PNG" } else { "JPEG" },
                    start.elapsed().as_secs_f64() * 1000.0,
                    100.0 * (a - b) as f64 / a as f64
                );
            }
            println!(
                "batch {preset}: {:.2} ms (including fixture generation)",
                batch.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
}

fn attributes(path: &Path) -> Result<Vec<(std::ffi::OsString, Vec<u8>)>> {
    let mut result = Vec::new();
    for name in xattr::list(path)? {
        // macOS owns per-inode provenance and per-app access grants (MACL).
        // Native app writes can regenerate/merge them even when setxattr reports
        // success. Leave these to the OS: never copy, remove, or compare them as
        // portable metadata. This does not skip filesystem ACL checks.
        #[cfg(target_os = "macos")]
        if name == "com.apple.provenance" || name == "com.apple.macl" {
            continue;
        }
        if name == "com.apple.ResourceFork" {
            return Err("File has a resource fork; original retained".into());
        }
        // POSIX ACLs require dedicated ownership-aware handling.
        if name.to_string_lossy().contains("posix_acl") {
            return Err("File has an ACL; original retained".into());
        }
        let value = xattr::get(path, &name)?.ok_or("Extended attributes changed")?;
        result.push((name, value));
    }
    result.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(result)
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    use image::ImageEncoder;
    #[test]
    fn transparency_icc_exif_and_animation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rgba.png");
        let mut data = Vec::new();
        let mut encoder = png::Encoder::new(&mut data, 3, 7);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .add_text_chunk("Author".into(), "Zapbeam test".into())
            .unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&[13, 27, 89, 128].repeat(21))
            .unwrap();
        writer.finish().unwrap();
        fs::write(&path, &data).unwrap();
        optimize(&path, Path::new("unused"), &Mutex::new(false), 2).unwrap();
        assert_eq!(
            decode(&data).unwrap().to_rgba16(),
            decode(&fs::read(&path).unwrap()).unwrap().to_rgba16()
        );
        assert_eq!(
            metadata_policy::png_expected(&data).unwrap(),
            png_metadata(&fs::read(&path).unwrap()).unwrap()
        );
        let mut animated = Vec::new();
        let mut enc = png::Encoder::new(&mut animated, 1, 1);
        enc.set_animated(1, 0).unwrap();
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(&[0]).unwrap();
        writer.finish().unwrap();
        fs::write(&path, &animated).unwrap();
        assert!(
            optimize(&path, Path::new("unused"), &Mutex::new(false), 2)
                .unwrap_err()
                .to_string()
                .contains("Animated")
        );
        // Minimal EXIF TIFF with orientation=6; ICC payload is preserved as opaque data.
        let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
        let mut jpeg = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new(&mut jpeg);
        encoder
            .set_icc_profile(b"test ICC marker payload".to_vec())
            .unwrap();
        encoder
            .encode(&[25; 3 * 13 * 19], 13, 19, image::ExtendedColorType::Rgb8)
            .unwrap();
        let mut marked = jpeg[..2].to_vec();
        marked.extend([255, 225]);
        marked.extend(((exif.len() + 2) as u16).to_be_bytes());
        marked.extend(exif);
        marked.extend(&jpeg[2..]);
        fs::write(&path, &marked).unwrap();
        let engine = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "binaries/jpegtran-{}",
            env!("TAURI_ENV_TARGET_TRIPLE")
        ));
        optimize(&path, &engine, &Mutex::new(false), 2).unwrap();
        assert_eq!(
            metadata_policy::jpeg_expected(&marked).unwrap(),
            jpeg_metadata(&fs::read(&path).unwrap()).unwrap()
        );
    }
    #[test]
    fn png_icc_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile.png");
        let mut bytes = Vec::new();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
        encoder
            .set_icc_profile(b"opaque ICC profile payload".repeat(30))
            .unwrap();
        encoder
            .write_image(&[80; 17 * 31 * 4], 17, 31, image::ExtendedColorType::Rgba8)
            .unwrap();
        fs::write(&path, &bytes).unwrap();
        optimize(&path, Path::new("unused"), &Mutex::new(false), 2).unwrap();
        assert_eq!(
            metadata_policy::png_expected(&bytes).unwrap(),
            png_metadata(&fs::read(&path).unwrap()).unwrap()
        );
    }
    #[test]
    fn queue_deduplicates_and_cancellation_removes_waiting_and_active() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("file");
        fs::write(&p, b"test").unwrap();
        let path = p.canonicalize().unwrap().to_str().unwrap().to_owned();
        let mut queue = Queue::default();
        enqueue(
            &mut queue,
            vec![
                Request {
                    id: "a".into(),
                    path: path.clone(),
                },
                Request {
                    id: "b".into(),
                    path,
                },
            ],
        )
        .unwrap();
        assert_eq!(queue.pending.len(), 1);
        let active = queue.pending.pop_front().unwrap();
        cancel_jobs(&mut queue, vec!["a".into()]);
        assert!(*active.cancelled.lock().unwrap());
        assert!(queue.jobs.is_empty());
        enqueue(&mut queue, vec![active.request.clone()]).unwrap();
        cancel_jobs(&mut queue, vec!["a".into()]);
        assert!(queue.pending.is_empty());
    }
}

fn optimize_png(source: &[u8], preset: u8) -> Result<Vec<u8>> {
    let mut options = oxipng::Options::from_preset(preset);
    options.interlace = None;
    options.optimize_alpha = false;
    options.strip = metadata_policy::png_options();
    // These chunks depend on palette entries, color type or bit depth. OxiPNG's
    // OxiPNG drops these even with Keep when reductions invalidate them.
    // Keep their representation intact while still optimizing filters/deflate.
    if metadata_policy::png_expected(source)?
        .iter()
        .any(|c| matches!(&c[4..8], b"bKGD" | b"sBIT"))
    {
        options.bit_depth_reduction = false;
        options.color_type_reduction = false;
        options.palette_reduction = false;
        options.grayscale_reduction = false;
    }
    options.timeout = Some(Duration::from_secs(30));
    let input = metadata_policy::png_input(source)?;
    let optimized = oxipng::optimize_from_memory(&input, &options)?;
    // OxiPNG can recompress ICC. Retain the original profile serialization,
    // including its CRC, rather than treating a different deflate stream as data loss.
    png_metadata(&optimized)?;
    let metadata = metadata_policy::png_expected(source)?;
    let mut compressed = metadata
        .iter()
        .filter(|chunk| matches!(&chunk[4..8], b"iCCP"));
    let mut result = optimized[..8].to_vec();
    let mut offset = 8;
    while offset < optimized.len() {
        let length = u32::from_be_bytes(optimized[offset..offset + 4].try_into()?) as usize;
        let end = offset + length + 12;
        let chunk = &optimized[offset..end];
        if matches!(&chunk[4..8], b"iCCP") {
            let original = compressed.next().ok_or("Unexpected output metadata")?;
            if original[4..8] != chunk[4..8] {
                return Err("Metadata order changed".into());
            }
            result.extend_from_slice(original);
        } else {
            result.extend_from_slice(chunk);
        }
        offset = end;
    }
    if compressed.next().is_some() {
        return Err("Output metadata missing".into());
    }
    Ok(result)
}
fn optimize_jpeg(
    source: &[u8],
    parent: &Path,
    candidate: &Path,
    jpegtran: &Path,
    cancel: &Mutex<bool>,
) -> Result<()> {
    let mut input = NamedTempFile::new_in(parent)?;
    input.write_all(&metadata_policy::jpeg_input(source)?)?;
    input.flush()?;
    let stderr = NamedTempFile::new_in(parent)?;
    // Use MozJPEG's default progressive scan optimization in one coefficient pass.
    let mut child = Command::new(jpegtran)
        .args([
            "-optimize",
            "-strict",
            "-maxscans",
            "100",
            "-copy",
            "all",
            "-maxmemory",
            "131072",
            "-outfile",
        ])
        .arg(candidate)
        .arg(input.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr.as_file().try_clone()?)
        .spawn()
        .map_err(|e| format!("Bundled JPEG engine could not start: {e}"))?;
    let start = Instant::now();
    loop {
        if check_cancel(cancel).is_err() || start.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            check_cancel(cancel)?;
            return Err("JPEG engine exceeded 30 second limit".into());
        }
        let status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.into());
            }
        };
        if let Some(status) = status {
            if !status.success() {
                let mut detail = String::new();
                fs::File::open(stderr.path())?
                    .take(4096)
                    .read_to_string(&mut detail)?;
                for private in [input.path(), candidate, jpegtran] {
                    detail = detail.replace(private.to_string_lossy().as_ref(), "<private-path>");
                }
                return Err(format!("JPEG engine {status}: {}", detail.trim()).into());
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

#[cfg(test)]
mod screenshot_tests {
    use super::*;
    #[test]
    fn content_credentials_are_retained_with_a_specific_error() {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_chunk(png::chunk::ChunkType(*b"caBX"), b"signed metadata")
            .unwrap();
        writer.write_image_data(&[1, 2, 3]).unwrap();
        writer.finish().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("signed.png");
        fs::write(&path, &bytes).unwrap();
        assert!(
            optimize(&path, Path::new("unused"), &Mutex::new(false), 2)
                .unwrap_err()
                .to_string()
                .contains("Content Credentials")
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn apple_offset_chunk_does_not_prevent_safe_compression() {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 257, 131);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_compression(png::Compression::Fast);
        encoder.set_filter(png::Filter::NoFilter);
        encoder
            .add_text_chunk("Author".into(), "Preserve this metadata".into())
            .unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_chunk(png::chunk::ChunkType(*b"iDOT"), &[0; 28])
            .unwrap();
        writer
            .write_image_data(&[32, 64, 128].repeat(257 * 131))
            .unwrap();
        writer.finish().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screenshot.png");
        fs::write(&path, &bytes).unwrap();
        let (before, after) = optimize(&path, Path::new("unused"), &Mutex::new(false), 2).unwrap();
        let output = fs::read(&path).unwrap();
        assert!(after < before);
        assert!(bytes.windows(4).any(|w| w == b"iDOT"));
        assert!(!output.windows(4).any(|w| w == b"iDOT"));
        assert_eq!(
            metadata_policy::png_expected(&bytes).unwrap(),
            png_metadata(&output).unwrap()
        );
        assert_eq!(
            decode(&bytes).unwrap().to_rgba16(),
            decode(&output).unwrap().to_rgba16()
        );
        assert!(
            png_metadata(&bytes)
                .unwrap()
                .iter()
                .any(|c| &c[4..8] == b"tEXt")
        );
    }
}

#[cfg(test)]
mod reliability_tests {
    use super::*;
    #[test]
    fn palette_background_survives_optimization() {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 64, 64);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]);
        encoder.set_compression(png::Compression::NoCompression);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_chunk(png::chunk::ChunkType(*b"bKGD"), &[3])
            .unwrap();
        writer.write_image_data(&[0, 1, 2, 3].repeat(1024)).unwrap();
        writer.finish().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("palette.png");
        fs::write(&path, &bytes).unwrap();
        let (a, b) = optimize(&path, Path::new("unused"), &Mutex::new(false), 2).unwrap();
        assert!(b < a);
        let output = fs::read(&path).unwrap();
        assert_eq!(
            metadata_policy::png_expected(&bytes).unwrap(),
            png_metadata(&output).unwrap()
        );
        assert!(same_pixels(
            &decode(&bytes).unwrap(),
            &decode(&output).unwrap()
        ));
        assert_eq!(&bytes[24..26], &output[24..26]);
    }

    #[test]
    fn failures_have_categories_and_engine_details_without_paths() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("private-image.jpg");
        super::tests::fixture(&path, false, 31, 17);
        let original = fs::read(&path).unwrap();
        let script = dir.path().join("engine");
        fs::write(
            &script,
            b"#!/bin/sh\necho 'confirmed engine failure' >&2\nexit 7\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let error = optimize(&path, &script, &Mutex::new(false), 2).unwrap_err();
        let error = error.downcast_ref::<Failure>().unwrap();
        assert_eq!(error.category, Category::Engine);
        assert_eq!(error.stage, "jpegtran");
        assert!(error.explanation().contains("confirmed engine failure"));
        assert!(error.explanation().contains('7'));
        assert!(!error.explanation().contains("private-image"));
        let error =
            optimize(&path, Path::new("/missing-engine"), &Mutex::new(false), 2).unwrap_err();
        assert!(error.to_string().contains("os error"));
        let error = optimize_inner(
            &path,
            &super::tests::engine(),
            &Mutex::new(false),
            2,
            || {
                fs::write(&path, b"external edit").unwrap();
            },
        )
        .unwrap_err();
        assert_eq!(
            error.downcast_ref::<Failure>().unwrap().category,
            Category::SourceChanged
        );
        assert_eq!(fs::read(&path).unwrap(), b"external edit");
        fs::write(&path, &original).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        let error = optimize(&path, &script, &Mutex::new(false), 2).unwrap_err();
        assert_eq!(
            error.downcast_ref::<Failure>().unwrap().category,
            Category::Filesystem
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        assert_eq!(
            serde_json::to_string(&Category::NoSavings).unwrap(),
            "\"noSavings\""
        );
    }

    #[test]
    #[ignore = "set ZAPBEAM_OUTPUT to a new directory; creates disposable native-app fixtures"]
    fn export_reliability_fixtures() {
        let root =
            PathBuf::from(std::env::var_os("ZAPBEAM_OUTPUT").expect("new fixture directory"));
        fs::create_dir(&root).expect("destination must not exist");
        for (name, png) in [("rgb.png", true), ("baseline.jpg", false)] {
            super::tests::fixture(&root.join(name), png, 640, 480);
            let optimized = root.join(format!("optimized-{name}"));
            fs::copy(root.join(name), &optimized).unwrap();
            optimize(&optimized, &super::tests::engine(), &Mutex::new(false), 2).unwrap();
        }
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 64, 64);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]);
        encoder.set_compression(png::Compression::NoCompression);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_chunk(png::chunk::ChunkType(*b"bKGD"), &[3])
            .unwrap();
        writer.write_image_data(&[0, 1, 2, 3].repeat(1024)).unwrap();
        writer.finish().unwrap();
        fs::write(root.join("palette.png"), bytes).unwrap();
        let mut signed = Vec::new();
        let mut encoder = png::Encoder::new(&mut signed, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_chunk(png::chunk::ChunkType(*b"caBX"), b"signed fixture")
            .unwrap();
        writer.write_image_data(&[20, 40, 60, 128]).unwrap();
        writer.finish().unwrap();
        fs::write(root.join("signed.png"), signed).unwrap();
        fs::write(root.join("malformed.png"), b"\x89PNG\r\n\x1a\ninvalid").unwrap();
        fs::copy(root.join("rgb.png"), root.join("readonly.png")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join("readonly.png"), fs::Permissions::from_mode(0o444)).unwrap();
        xattr::set(root.join("rgb.png"), "user.zapbeam-test", b"preserved").unwrap();
    }

    #[test]
    #[ignore = "read-only verification: set ZAPBEAM_FIXTURES and ZAPBEAM_OUTPUT"]
    fn verify_desktop_corpus() {
        let source = PathBuf::from(std::env::var_os("ZAPBEAM_FIXTURES").expect("original corpus"));
        let output = PathBuf::from(std::env::var_os("ZAPBEAM_OUTPUT").expect("processed copies"));
        let mut count = 0;
        for entry in fs::read_dir(&source).unwrap() {
            let entry = entry.unwrap();
            let a = entry.path();
            let b = output.join(entry.file_name());
            let before = fs::read(&a).unwrap();
            let after = fs::read(&b).unwrap();
            assert!(after.len() <= before.len());
            if before != after {
                assert!(after.len() < before.len());
                assert!(same_pixels(
                    &decode(&before).unwrap(),
                    &decode(&after).unwrap()
                ));
                let metadata = if before.starts_with(b"\x89PNG") {
                    png_metadata
                } else {
                    jpeg_metadata
                };
                let expected = if before.starts_with(b"\x89PNG") {
                    metadata_policy::png_expected(&before)
                } else {
                    metadata_policy::jpeg_expected(&before)
                };
                assert_eq!(expected.unwrap(), metadata(&after).unwrap());
            }
            let am = fs::metadata(&a).unwrap();
            let bm = fs::metadata(&b).unwrap();
            assert_eq!(am.mode(), bm.mode());
            assert_eq!((am.mtime(), am.mtime_nsec()), (bm.mtime(), bm.mtime_nsec()));
            assert_eq!(attributes(&a).unwrap(), attributes(&b).unwrap());
            println!(
                "{}: {} -> {} (integrity/metadata OK)",
                entry.file_name().to_string_lossy(),
                before.len(),
                after.len()
            );
            count += 1;
        }
        assert_eq!(
            fs::read_dir(output).unwrap().count(),
            count,
            "unexpected temporary files"
        );
    }

    /// Read a local corpus, process ONLY newly created temporary copies, print outcomes.
    #[test]
    #[ignore = "set ZAPBEAM_FIXTURES to a disposable corpus directory"]
    fn reliability_corpus() {
        let root = PathBuf::from(std::env::var_os("ZAPBEAM_FIXTURES").expect("fixture directory"));
        let engine = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "binaries/jpegtran-{}",
            env!("TAURI_ENV_TARGET_TRIPLE")
        ));
        let mut entries: Vec<_> = fs::read_dir(root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for original in entries {
            if !original.is_file() {
                continue;
            }
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("copy");
            fs::copy(&original, &path).unwrap();
            for (name, value) in attributes(&original).unwrap() {
                xattr::set(&path, name, &value).unwrap();
            }
            let bytes = fs::read(&path).unwrap();
            let start = Instant::now();
            let result = optimize(&path, &engine, &Mutex::new(false), 2);
            println!(
                "{}: {:?} ({:?})",
                original.file_name().unwrap().to_string_lossy(),
                result,
                start.elapsed()
            );
            let output = fs::read(&path).unwrap();
            match result {
                Ok((a, b)) => {
                    assert!(b <= a);
                    assert!(same_pixels(
                        &decode(&bytes).unwrap(),
                        &decode(&output).unwrap()
                    ));
                    if a == b {
                        assert_eq!(bytes, output);
                    }
                }
                Err(_) => assert_eq!(bytes, output),
            }
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
}

#[cfg(test)]
mod comparison {
    use super::*;
    #[test]
    #[ignore = "process fresh copies: ZAPBEAM_FIXTURES and new ZAPBEAM_OUTPUT directory"]
    fn comparison_corpus() {
        let source = PathBuf::from(std::env::var_os("ZAPBEAM_FIXTURES").unwrap());
        let output = PathBuf::from(std::env::var_os("ZAPBEAM_OUTPUT").unwrap());
        fs::create_dir(&output).expect("output must be new");
        let mut entries: Vec<_> = fs::read_dir(source)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        let mut rows = Vec::new();
        for original in entries.into_iter().filter(|p| p.is_file()) {
            let path = output.join(original.file_name().unwrap());
            fs::copy(&original, &path).unwrap();
            let bytes = fs::read(&original).unwrap();
            let start = Instant::now();
            let result = optimize(&path, &tests::engine(), &Mutex::new(false), 2);
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            let after = fs::read(&path).unwrap();
            let status = match &result {
                Ok((a, b)) if b < a => "optimized",
                Ok(_) => "unchanged",
                Err(e)
                    if e.downcast_ref::<Failure>()
                        .is_some_and(|f| f.category == Category::Unsupported) =>
                {
                    "unsupported"
                }
                Err(_) => "error",
            };
            if result.is_err() || bytes.len() == after.len() {
                assert_eq!(bytes, after);
            }
            assert_eq!(bytes, fs::read(&original).unwrap());
            rows.push(serde_json::json!({"file":original.file_name().unwrap().to_string_lossy(),
                "original":bytes.len(), "final":after.len(), "savings_percent":100.0*(1.0-after.len() as f64/bytes.len() as f64),
                "duration_ms":elapsed, "status":status, "error":result.err().map(|e| e.to_string())}));
        }
        fs::write(
            output.with_extension("json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}
