//! Local, session-only tools over merged kernels. These do not create evidence/case records.
use oia_documents::{
    ExtractionLimits, extract_utf8,
    search::{SearchLimits, find_text},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

mod forensics;
mod privacy;
pub use forensics::{ForensicView, SNAPSHOT_SCHEME, analyze_image, media_type, residual_map};
pub use privacy::{PrivacyView, Selection, clean_image, inspect_image};

pub const TEXT_BYTES: usize = 2 * 1024 * 1024;
pub const IMAGE_BYTES: usize = 16 * 1024 * 1024;
pub const PASSAGES: usize = 10_000;
pub const MATCHES: usize = 200;

#[derive(Clone, Debug, Serialize)]
pub struct ToolError {
    pub code: &'static str,
    pub message: String,
}
impl ToolError {
    pub fn new(code: &'static str, message: impl ToString) -> Self {
        Self {
            code,
            message: message.to_string(),
        }
    }
}
impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ToolError {}
pub type Result<T> = std::result::Result<T, ToolError>;

/// Cooperative cancellation and deadline for one bounded operation, including publication.
#[derive(Clone, Debug)]
pub struct Control {
    cancelled: Arc<AtomicBool>,
    started: Instant,
}
impl Default for Control {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            started: Instant::now(),
        }
    }
}
impl Control {
    /// Dialog time is user interaction, not processing time. Cancellation stays shared.
    pub fn processing(&self) -> Self {
        Self {
            cancelled: self.cancelled.clone(),
            started: Instant::now(),
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Acquire) {
            Err(ToolError::new(
                "cancelled",
                "Operation cancelled; no result applied.",
            ))
        } else if self.started.elapsed() > Duration::from_secs(30) {
            Err(ToolError::new(
                "deadline",
                "Operation exceeded its 30-second processing limit.",
            ))
        } else {
            Ok(())
        }
    }
    fn text_check(&self) -> std::result::Result<(), oia_documents::StopReason> {
        self.check().map_err(|e| {
            if e.code == "cancelled" {
                oia_documents::StopReason::Cancelled
            } else {
                oia_documents::StopReason::DeadlineExceeded
            }
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Text,
    Image,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub bytes: usize,
    pub sha256: String,
    pub kind: FileKind,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub info: FileInfo,
    bytes: Arc<Vec<u8>>,
}
impl Snapshot {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

pub fn open_file(path: &Path, kind: FileKind, control: &Control) -> Result<Snapshot> {
    control.check()?;
    let limit = match kind {
        FileKind::Text => TEXT_BYTES,
        FileKind::Image => IMAGE_BYTES,
    };
    let file = File::open(path).map_err(|e| ToolError::new("file_read", e))?;
    let metadata = file
        .metadata()
        .map_err(|e| ToolError::new("file_read", e))?;
    if !metadata.is_file() {
        return Err(ToolError::new("file_type", "Choose a regular local file."));
    }
    if metadata.len() > limit as u64 {
        return Err(ToolError::new(
            "input_limit",
            format!("File exceeds the {limit}-byte limit; nothing was loaded."),
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ToolError::new("file_read", e))?;
    if bytes.len() > limit {
        return Err(ToolError::new(
            "input_limit",
            "File grew beyond the read limit; nothing was loaded.",
        ));
    }
    control.check()?;
    let info = FileInfo {
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path: path.to_string_lossy().into_owned(),
        bytes: bytes.len(),
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        kind,
    };
    Ok(Snapshot {
        info,
        bytes: Arc::new(bytes),
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct Passage {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub text: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct DocumentView {
    pub file: FileInfo,
    pub passages: Vec<Passage>,
    pub extracted_bytes: usize,
    pub complete: bool,
    pub passage_limit: usize,
}

// Escape hidden controls for display only. Source bytes and anchors remain unchanged.
fn display_text(text: &str) -> String {
    let mut displayed = String::with_capacity(text.len());
    for c in text.chars() {
        if (c.is_control() && c != '\n' && c != '\t')
            || matches!(c, '\u{061c}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
        {
            displayed.extend(c.escape_unicode());
        } else {
            displayed.push(c);
        }
    }
    displayed
}

pub fn document(snapshot: &Snapshot, control: &Control) -> Result<DocumentView> {
    require_kind(snapshot, FileKind::Text)?;
    let limits =
        ExtractionLimits::new(TEXT_BYTES, PASSAGES).map_err(|e| ToolError::new("limits", e))?;
    let extraction = extract_utf8(snapshot.bytes(), limits, || control.text_check())
        .map_err(|e| ToolError::new("extraction", e))?;
    let passages = extraction
        .passages()
        .iter()
        .map(|p| Passage {
            line: p.line_number(),
            start: p.byte_range().start,
            end: p.byte_range().end,
            text: display_text(p.text()),
        })
        .collect();
    control.check()?;
    Ok(DocumentView {
        file: snapshot.info.clone(),
        passages,
        extracted_bytes: extraction.coverage().extracted_bytes(),
        complete: extraction.coverage().unprocessed_range().is_none(),
        passage_limit: PASSAGES,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct SearchView {
    pub sha256: String,
    pub query: String,
    pub matches: Vec<Passage>,
    pub searched_bytes: usize,
    pub input_bytes: usize,
    pub complete: bool,
    pub match_limit: usize,
}
pub fn search(snapshot: &Snapshot, query: &str, control: &Control) -> Result<SearchView> {
    require_kind(snapshot, FileKind::Text)?;
    let limits =
        ExtractionLimits::new(TEXT_BYTES, PASSAGES).map_err(|e| ToolError::new("limits", e))?;
    let extraction = extract_utf8(snapshot.bytes(), limits, || control.text_check())
        .map_err(|e| ToolError::new("extraction", e))?;
    let limits = SearchLimits::new(512, MATCHES).map_err(|e| ToolError::new("limits", e))?;
    let result = find_text(&extraction, query, limits, || control.text_check())
        .map_err(|e| ToolError::new("search", e))?;
    let matches = result
        .matches()
        .iter()
        .map(|m| Passage {
            line: m.line_number(),
            start: m.byte_range().start,
            end: m.byte_range().end,
            text: display_text(m.text()),
        })
        .collect();
    control.check()?;
    Ok(SearchView {
        sha256: snapshot.info.sha256.clone(),
        query: query.to_owned(),
        matches,
        searched_bytes: result.coverage().searched_bytes(),
        input_bytes: snapshot.info.bytes,
        complete: result.coverage().is_complete(),
        match_limit: MATCHES,
    })
}

pub(super) fn require_kind(snapshot: &Snapshot, kind: FileKind) -> Result<()> {
    if snapshot.info.kind != kind {
        Err(ToolError::new(
            "file_type",
            "Open a file in this tool first.",
        ))
    } else {
        Ok(())
    }
}

/// Stage beside the destination, then publish without replacing any existing path.
/// Dropping an uncommitted export removes the staging file.
#[derive(Debug)]
pub struct PreparedExport {
    temporary: tempfile::NamedTempFile,
    destination: PathBuf,
}
impl PreparedExport {
    pub fn prepare(destination: &Path, bytes: &[u8], control: &Control) -> Result<Self> {
        control.check()?;
        if destination.symlink_metadata().is_ok() {
            return Err(ToolError::new(
                "destination_exists",
                "Choose a new filename. Existing files are never overwritten.",
            ));
        }
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|e| ToolError::new("export", e))?;
        temporary
            .write_all(bytes)
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|e| ToolError::new("export", e))?;
        control.check()?;
        Ok(Self {
            temporary,
            destination: destination.to_owned(),
        })
    }
    /// Caller serializes this commit with cancellation/publication of the active operation.
    pub fn commit(self, control: &Control) -> Result<PathBuf> {
        control.check()?;
        self.temporary
            .persist_noclobber(&self.destination)
            .map_err(|e| ToolError::new("export", e.error))?;
        Ok(self.destination)
    }
}
