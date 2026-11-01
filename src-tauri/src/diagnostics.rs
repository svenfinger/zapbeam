//! Local, bounded JSONL diagnostics. Never record source paths, names or metadata values.
use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Inspected,
    Queued,
    Compressed,
    NoSavings,
    Unsupported,
    Filesystem,
    Engine,
    InputValidation,
    OutputValidation,
    SourceChanged,
    Cancelled,
}
#[derive(Debug)]
pub struct Failure {
    pub category: Category,
    pub stage: &'static str,
    pub cause: Error,
}
impl Failure {
    pub fn explanation(&self) -> String {
        let mut cause = self.cause.as_ref();
        while let Some(nested) = cause.downcast_ref::<Failure>() {
            cause = nested.cause.as_ref();
        }
        let label = match self.category {
            Category::Unsupported => "Unsupported image",
            Category::Filesystem => "File access failed",
            Category::Engine => "Compression failed",
            Category::InputValidation => "Invalid image",
            Category::OutputValidation => "Output validation failed",
            Category::SourceChanged => "Source changed",
            Category::Cancelled => "Cancelled",
            _ => "Processing failed",
        };
        format!("{label}: {cause}")
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.stage, self.cause)
    }
}
impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.cause.as_ref())
    }
}
pub fn failure(category: Category, message: &'static str) -> Error {
    Box::new(Failure {
        category,
        stage: "validation",
        cause: message.into(),
    })
}
static LOG: OnceLock<Mutex<PathBuf>> = OnceLock::new();
static SEQUENCE: AtomicU64 = AtomicU64::new(1);
pub fn init(directory: PathBuf) -> std::io::Result<()> {
    fs::create_dir_all(&directory)?;
    let path = directory.join("compression.jsonl");
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)?;
    let _ = LOG.set(Mutex::new(path));
    Ok(())
}
fn record(value: serde_json::Value) {
    let Some(log) = LOG.get() else { return };
    let Ok(path) = log.lock() else { return };
    let write = || -> std::io::Result<()> {
        if fs::metadata(&*path)?.len() > 2 * 1024 * 1024 {
            fs::rename(&*path, path.with_extension("previous.jsonl"))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&*path)?;
        writeln!(file, "{value}")
    };
    if let Err(error) = write() {
        eprintln!("Compression diagnostics unavailable: {error}");
    }
}
pub struct Trace {
    id: u64,
    start: Instant,
    stage_start: Instant,
    stage: &'static str,
    category: Category,
    pub format: Option<&'static str>,
    pub source_size: Option<u64>,
    pub candidate_size: Option<u64>,
}
impl Trace {
    pub fn new() -> Self {
        Self {
            id: SEQUENCE.fetch_add(1, Ordering::Relaxed),
            start: Instant::now(),
            stage_start: Instant::now(),
            stage: "",
            category: Category::InputValidation,
            format: None,
            source_size: None,
            candidate_size: None,
        }
    }
    fn event(&self, outcome: &str, category: Option<Category>, error: Option<&str>) {
        record(serde_json::json!({
            "timestampMs": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "pid": std::process::id(), "job": self.id, "build": if cfg!(debug_assertions) { "debug" } else { "release" },
            "stage": self.stage, "outcome": outcome, "stageMs": self.stage_start.elapsed().as_millis(),
            "totalMs": self.start.elapsed().as_millis(), "format": self.format,
            "sourceBytes": self.source_size, "candidateBytes": self.candidate_size,
            "category": category, "error": error,
        }));
    }
    pub fn stage(&mut self, stage: &'static str, category: Category) {
        if !self.stage.is_empty() {
            self.event("ok", None, None);
        }
        self.stage = stage;
        self.category = category;
        self.stage_start = Instant::now();
        self.event("started", None, None);
    }
    pub fn finish<T>(&self, result: Result<T>, success: Category) -> Result<T> {
        match result {
            Ok(value) => {
                self.event("finished", Some(success), None);
                Ok(value)
            }
            Err(error) => {
                let category = error
                    .downcast_ref::<Failure>()
                    .map_or(self.category, |e| e.category);
                // Callers use OS errors and codec messages, never source paths or metadata payloads.
                self.event("failed", Some(category), Some(&error.to_string()));
                Err(Box::new(Failure {
                    category,
                    stage: self.stage,
                    cause: error,
                }))
            }
        }
    }
}
