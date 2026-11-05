use std::fmt;
use std::path::Path;
use std::time::Duration;

use oia_domain::evidence::{
    CodecError, EvidenceLedger, LedgerEntry, LedgerLimits, MAX_LEDGER_JSON_BYTES,
};
use rusqlite::limits::Limit;
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};

const APPLICATION_ID: i64 = 0x4f49_414c; // OIAL, distinct from the domain wire schema.
const STORE_SCHEMA: i64 = 1;

/// Resource controls applied before parsing stored or incoming ledger bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreLimits {
    /// UTF-8 JSON bytes per ledger, at most the domain codec's 64 MiB ceiling.
    pub max_ledger_bytes: usize,
    /// Domain record/text/relation/flag limits; used on every save and load.
    pub ledger: LedgerLimits,
}

impl Default for StoreLimits {
    fn default() -> Self {
        Self {
            max_ledger_bytes: 8 * 1024 * 1024,
            ledger: LedgerLimits::default(),
        }
    }
}

/// A consistent, validated snapshot. Revision zero is the initial empty ledger.
#[derive(Debug)]
pub struct StoredLedger {
    pub revision: u64,
    pub ledger: EvidenceLedger,
}

/// Actionable failures. Any failed save leaves the committed snapshot unchanged.
#[derive(Debug)]
pub enum StoreError {
    InvalidLimits,
    TooLarge {
        bytes: usize,
        limit: usize,
    },
    InvalidLedger(CodecError),
    ForeignDatabase {
        application_id: i64,
    },
    UnsupportedSchema {
        found: i64,
        supported: i64,
    },
    InvalidLayout(&'static str),
    Conflict {
        expected: u64,
        actual: u64,
    },
    HistoryChanged {
        entry: usize,
    },
    RevisionExhausted,
    /// Inspect the SQLite error code for busy/locked, disk-full, read-only or I/O failures.
    Database(rusqlite::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => {
                write!(f, "ledger byte limit must be 1..={MAX_LEDGER_JSON_BYTES}")
            }
            Self::TooLarge { bytes, limit } => {
                write!(f, "ledger is {bytes} bytes; configured limit is {limit}")
            }
            Self::InvalidLedger(error) => write!(f, "ledger validation failed: {error}"),
            Self::ForeignDatabase { application_id } => {
                write!(
                    f,
                    "database is not an OIA ledger (application id {application_id})"
                )
            }
            Self::UnsupportedSchema { found, supported } => {
                write!(
                    f,
                    "unsupported store schema {found}; supported schema is {supported}"
                )
            }
            Self::InvalidLayout(reason) => write!(f, "invalid OIA ledger database: {reason}"),
            Self::Conflict { expected, actual } => write!(
                f,
                "save revision {expected} is stale; current revision is {actual}; reload before appending"
            ),
            Self::HistoryChanged { entry } => {
                write!(f, "save removes or changes committed ledger entry {entry}")
            }
            Self::RevisionExhausted => write!(f, "store revision counter is exhausted"),
            Self::Database(error) => write!(f, "SQLite operation failed: {error}"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidLedger(error) => Some(error),
            Self::Database(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

impl From<CodecError> for StoreError {
    fn from(error: CodecError) -> Self {
        Self::InvalidLedger(error)
    }
}

/// One local database, containing one ledger. Paths are local configuration, never source data.
///
/// Use only on a local filesystem supporting SQLite WAL locks. All access-policy references
/// remain data: application services must authorize calls before using this adapter.
#[derive(Debug)]
pub struct LedgerStore {
    connection: Connection,
    limits: StoreLimits,
}

impl LedgerStore {
    /// Creates an initial schema on an empty database or validates an existing store.
    ///
    /// Unknown databases/schemas are refused before schema or journal-mode changes. Opening
    /// an existing store never repairs or discards malformed evidence. Parent directories
    /// must exist. Lock waits are limited to two seconds; errors retain SQLite's error code.
    pub fn open(path: impl AsRef<Path>, limits: StoreLimits) -> Result<Self, StoreError> {
        Self::open_inner(path.as_ref(), limits, true)
    }

    /// Opens an existing store without creating a database or initializing an empty one.
    pub fn open_existing(path: impl AsRef<Path>, limits: StoreLimits) -> Result<Self, StoreError> {
        Self::open_inner(path.as_ref(), limits, false)
    }

    fn open_inner(path: &Path, limits: StoreLimits, create: bool) -> Result<Self, StoreError> {
        if limits.max_ledger_bytes == 0 || limits.max_ledger_bytes > MAX_LEDGER_JSON_BYTES {
            return Err(StoreError::InvalidLimits);
        }
        let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        if create {
            flags |= OpenFlags::SQLITE_OPEN_CREATE;
        }
        let connection = Connection::open_with_flags(path, flags)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        // Permit the configured payload plus SQLite row/schema overhead, without an unbounded
        // SQLite allocation for a corrupted record. The public ceiling fits in i32.
        let sqlite_limit =
            i32::try_from(limits.max_ledger_bytes + 4096).map_err(|_| StoreError::InvalidLimits)?;
        connection.set_limit(Limit::SQLITE_LIMIT_LENGTH, sqlite_limit)?;
        connection.execute_batch("PRAGMA trusted_schema = OFF; PRAGMA foreign_keys = ON;")?;
        check_identity(&connection, create)?;
        let mode: String = connection.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(StoreError::InvalidLayout("WAL mode is unavailable"));
        }
        connection
            .execute_batch("PRAGMA synchronous = FULL; PRAGMA journal_size_limit = 8388608;")?;
        let mut store = Self { connection, limits };
        let transaction = store
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if check_identity(&transaction, create)? {
            let empty = EvidenceLedger::new(limits.ledger).to_json()?;
            check_size(empty.len(), limits)?;
            transaction.execute_batch(
                "CREATE TABLE ledger_snapshot (
                    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                    revision INTEGER NOT NULL CHECK (revision >= 0),
                    payload BLOB NOT NULL CHECK (typeof(payload) = 'blob')
                 ) STRICT;
                 PRAGMA application_id = 1330200908;
                 PRAGMA user_version = 1;",
            )?;
            transaction.execute(
                "INSERT INTO ledger_snapshot VALUES (1, 0, ?1)",
                [empty.as_bytes()],
            )?;
        }
        // Validate inside the same transaction, including before committing initial schema.
        read_snapshot(&transaction, limits)?;
        transaction.commit()?;
        Ok(store)
    }

    /// Reloads a transactionally consistent snapshot and revalidates its history.
    pub fn load(&mut self) -> Result<StoredLedger, StoreError> {
        let transaction = self.connection.transaction()?;
        check_identity(&transaction, false)?;
        let snapshot = read_snapshot(&transaction, self.limits)?;
        transaction.commit()?;
        Ok(snapshot)
    }

    /// Atomically appends a complete ledger document, retaining all committed entries exactly.
    ///
    /// The caller starts from [`Self::load`], appends through the domain API, serializes with
    /// `to_json`, and supplies the loaded revision. Limits also apply to foreign JSON. An
    /// equivalent ledger is a no-op (no revision increment). Stale revisions are always refused.
    /// The new document's exact UTF-8 bytes are retained only after full validation succeeds.
    pub fn append_json(&mut self, expected: u64, json: &str) -> Result<u64, StoreError> {
        check_size(json.len(), self.limits)?;
        let candidate = EvidenceLedger::from_json(json, self.limits.ledger)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        check_identity(&transaction, false)?;
        let current = read_snapshot(&transaction, self.limits)?;
        if expected != current.revision {
            return Err(StoreError::Conflict {
                expected,
                actual: current.revision,
            });
        }
        preserve_history(&current.ledger, &candidate)?;
        if current.ledger.entries().len() == candidate.entries().len() {
            transaction.commit()?;
            return Ok(current.revision);
        }
        let next = current
            .revision
            .checked_add(1)
            .and_then(|v| i64::try_from(v).ok())
            .ok_or(StoreError::RevisionExhausted)?;
        transaction.execute(
            "UPDATE ledger_snapshot SET revision = ?1, payload = ?2 WHERE singleton = 1",
            params![next, json.as_bytes()],
        )?;
        transaction.commit()?;
        u64::try_from(next).map_err(|_| StoreError::RevisionExhausted)
    }
}

fn check_size(bytes: usize, limits: StoreLimits) -> Result<(), StoreError> {
    if bytes > limits.max_ledger_bytes {
        Err(StoreError::TooLarge {
            bytes,
            limit: limits.max_ledger_bytes,
        })
    } else {
        Ok(())
    }
}

/// Returns true only for an uninitialized, empty SQLite database.
fn check_identity(connection: &Connection, allow_empty: bool) -> Result<bool, StoreError> {
    let app: i64 = connection.query_row("PRAGMA application_id", [], |r| r.get(0))?;
    let schema: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if app == 0 && schema == 0 && allow_empty {
        let objects: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )?;
        if objects == 0 {
            return Ok(true);
        }
    }
    if app != APPLICATION_ID {
        return Err(StoreError::ForeignDatabase {
            application_id: app,
        });
    }
    if schema != STORE_SCHEMA {
        return Err(StoreError::UnsupportedSchema {
            found: schema,
            supported: STORE_SCHEMA,
        });
    }
    Ok(false)
}

fn read_snapshot(connection: &Connection, limits: StoreLimits) -> Result<StoredLedger, StoreError> {
    let mut statement =
        connection.prepare("SELECT singleton, revision, payload FROM ledger_snapshot")?;
    let mut rows = statement.query([])?;
    let row = rows
        .next()?
        .ok_or(StoreError::InvalidLayout("missing snapshot"))?;
    let singleton: i64 = row.get(0)?;
    let revision: i64 = row.get(1)?;
    if singleton != 1 || revision < 0 {
        return Err(StoreError::InvalidLayout(
            "invalid snapshot identity or revision",
        ));
    }
    let bytes = row
        .get_ref(2)?
        .as_blob()
        .map_err(|_| StoreError::InvalidLayout("snapshot payload is not a blob"))?;
    check_size(bytes.len(), limits)?;
    let text = std::str::from_utf8(bytes)
        .map_err(|_| StoreError::InvalidLayout("snapshot is not UTF-8"))?;
    let ledger = EvidenceLedger::from_json(text, limits.ledger)?;
    if rows.next()?.is_some() {
        return Err(StoreError::InvalidLayout("multiple snapshots"));
    }
    Ok(StoredLedger {
        revision: u64::try_from(revision)
            .map_err(|_| StoreError::InvalidLayout("negative revision"))?,
        ledger,
    })
}

fn preserve_history(previous: &EvidenceLedger, next: &EvidenceLedger) -> Result<(), StoreError> {
    for (index, entry) in previous.entries().iter().enumerate() {
        let same = next.entries().get(index) == Some(entry)
            && match entry {
                LedgerEntry::Record(version) => previous.version(version) == next.version(version),
                LedgerEntry::Relation(id) => previous.relation(id) == next.relation(id),
            };
        if !same {
            return Err(StoreError::HistoryChanged { entry: index });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
