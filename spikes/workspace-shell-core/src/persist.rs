//! Shell context persistence (A01 slice: context survives restart).
//!
//! One bounded JSON file, written to a temporary sibling and renamed into
//! place so a crash mid-write leaves the previous file intact. Unreadable or
//! oversized files are reported, never silently replaced with defaults
//! without telling the user.

use std::fmt;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::jobs::JobRegistry;
use crate::shell::ShellState;

/// Schema version of the persisted file.
pub const PERSIST_SCHEMA: u32 = 1;

/// Upper bound on the state file size before parsing.
pub const MAX_STATE_BYTES: u64 = 1 << 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Persisted {
    pub schema: u32,
    pub saved_at_unix: u64,
    pub shell: ShellState,
    pub jobs: JobRegistry,
}

impl Persisted {
    #[must_use]
    pub fn new(shell: ShellState, jobs: JobRegistry) -> Self {
        Self {
            schema: PERSIST_SCHEMA,
            saved_at_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            shell,
            jobs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistError {
    Io(String),
    TooLarge { bytes: u64, limit: u64 },
    Malformed(String),
    SchemaMismatch { found: u32, expected: u32 },
}

impl fmt::Display for PersistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "state file I/O error: {e}"),
            Self::TooLarge { bytes, limit } => {
                write!(
                    f,
                    "state file is {bytes} bytes, above the {limit} byte limit"
                )
            }
            Self::Malformed(e) => write!(f, "state file is malformed: {e}"),
            Self::SchemaMismatch { found, expected } => {
                write!(
                    f,
                    "state file schema {found} differs from expected {expected}"
                )
            }
        }
    }
}

impl std::error::Error for PersistError {}

/// What happened to the previous shell context on startup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RestoreReport {
    /// No previous state file existed.
    Fresh,
    /// Previous state restored; active jobs became `interrupted`.
    Restored {
        saved_at_unix: u64,
        interrupted_jobs: u32,
    },
    /// A state file existed but could not be used; defaults are in effect and
    /// the unreadable file was left in place.
    Unreadable { reason: String },
}

/// Loads the state file if present. Missing is `Ok(None)`; anything else that
/// prevents use is an error with the reason.
pub fn load(path: &Path) -> Result<Option<Persisted>, PersistError> {
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(PersistError::Io(e.to_string())),
    };
    if metadata.len() > MAX_STATE_BYTES {
        return Err(PersistError::TooLarge {
            bytes: metadata.len(),
            limit: MAX_STATE_BYTES,
        });
    }
    let bytes = fs::read(path).map_err(|e| PersistError::Io(e.to_string()))?;
    let persisted: Persisted =
        serde_json::from_slice(&bytes).map_err(|e| PersistError::Malformed(e.to_string()))?;
    if persisted.schema != PERSIST_SCHEMA {
        return Err(PersistError::SchemaMismatch {
            found: persisted.schema,
            expected: PERSIST_SCHEMA,
        });
    }
    Ok(Some(persisted))
}

/// Writes the state atomically: temporary sibling, flush and sync, rename.
pub fn save(path: &Path, persisted: &Persisted) -> Result<(), PersistError> {
    let json =
        serde_json::to_vec_pretty(persisted).map_err(|e| PersistError::Malformed(e.to_string()))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| PersistError::Io(e.to_string()))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp_path = Path::new(&tmp);
    {
        let mut file = fs::File::create(tmp_path).map_err(|e| PersistError::Io(e.to_string()))?;
        file.write_all(&json)
            .map_err(|e| PersistError::Io(e.to_string()))?;
        file.sync_all()
            .map_err(|e| PersistError::Io(e.to_string()))?;
    }
    // std::fs::rename replaces an existing destination on Windows and Unix,
    // so the previous file is never missing: either the old or the new one is
    // present at `path`.
    fs::rename(tmp_path, path).map_err(|e| PersistError::Io(e.to_string()))
}
