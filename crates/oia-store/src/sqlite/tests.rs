//! Critical contracts: retained history/concurrent saves, interrupted/full-disk rollback,
//! and refusal of incompatible or malformed persisted data. Fixtures are synthetic.
use std::error::Error;
use std::process::Command;

use oia_domain::evidence::{AttributeName, AttributeValue, FieldValue};
use oia_domain::{OpaqueId, VersionId};
use rusqlite::ErrorCode;

use super::*;

type TestResult = Result<(), Box<dyn Error>>;
const FIXTURE: &str = include_str!("../../../../fixtures/domain/evidence-ledger-correction.json");

fn fixture() -> String {
    FIXTURE.replace("\r\n", "\n")
}

#[test]
fn immutable_history_reopen_and_concurrent_saves() -> TestResult {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("evidence.db");
    let limits = StoreLimits::default();
    let original = fixture();
    let mut store = LedgerStore::open(&path, limits)?;
    let mut other = LedgerStore::open_existing(&path, limits)?;
    assert_eq!(other.load()?.revision, 0);
    assert_eq!(store.append_json(0, &original)?, 1);
    assert!(matches!(
        other.append_json(0, &original),
        Err(StoreError::Conflict {
            expected: 0,
            actual: 1
        })
    ));
    assert_eq!(other.load()?.ledger.to_json()?, original);
    assert_eq!(
        other.append_json(1, &original)?,
        1,
        "no-op advanced revision"
    );

    // A valid alternate document with the same IDs must not rewrite retained evidence.
    let changed = original.replace("Residents reported", "Invented observation");
    assert!(matches!(
        other.append_json(1, &changed),
        Err(StoreError::HistoryChanged { entry: 0 })
    ));
    let empty = EvidenceLedger::new(limits.ledger).to_json()?;
    assert!(matches!(
        other.append_json(1, &empty),
        Err(StoreError::HistoryChanged { .. })
    ));
    assert!(matches!(
        other.append_json(1, "{}"),
        Err(StoreError::InvalidLedger(_))
    ));
    assert!(matches!(
        other.append_json(1, &" ".repeat(limits.max_ledger_bytes + 1)),
        Err(StoreError::TooLarge { .. })
    ));
    drop(other);
    drop(store);
    let mut reopened = LedgerStore::open_existing(&path, limits)?;
    let snapshot = reopened.load()?;
    assert_eq!(snapshot.revision, 1);
    assert_eq!(
        snapshot.ledger.to_json()?,
        original,
        "flags, unknowns or wording changed"
    );
    let bytes: Vec<u8> =
        reopened
            .connection
            .query_row("SELECT payload FROM ledger_snapshot", [], |r| r.get(0))?;
    assert_eq!(bytes, original.as_bytes(), "stored UTF-8 bytes changed");
    let mut next = snapshot.ledger;
    let mut added = next
        .version(&VersionId::new("acct:witness-1:v1")?)
        .ok_or("fixture witness missing")?
        .fields()
        .clone();
    added.id = OpaqueId::new("acct:later")?;
    added.version = VersionId::new("acct:later:v1")?;
    next.issue(added)?;
    let extended = next.to_json()?;
    assert_eq!(reopened.append_json(1, &extended)?, 2);
    drop(reopened);
    let mut reopened = LedgerStore::open_existing(&path, limits)?;
    assert_eq!(reopened.load()?.revision, 2);
    assert_eq!(reopened.load()?.ledger.to_json()?, extended);
    Ok(())
}

#[test]
fn wal_recovery_and_storage_exhaustion() -> TestResult {
    const CHILD: &str = "OIA_SQLITE_INTERRUPTION_FIXTURE";
    if let Some(path) = std::env::var_os(CHILD) {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA cache_size = 4;
             BEGIN IMMEDIATE;
             UPDATE ledger_snapshot SET revision = 999, payload = zeroblob(1000000);",
        )?;
        // A spilled, uncommitted WAL is left behind. Exit bypasses connection/transaction Drop.
        let wal: String = connection.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        assert_eq!(wal, "wal");
        std::process::exit(0);
    }
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("interrupted.db");
    let limits = StoreLimits::default();
    let original = fixture();
    let mut store = LedgerStore::open(&path, limits)?;
    store.append_json(0, &original)?;
    drop(store);
    let status = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "sqlite::tests::wal_recovery_and_storage_exhaustion",
            "--nocapture",
        ])
        .env(CHILD, &path)
        .status()?;
    assert!(
        status.success(),
        "interruption fixture did not reach abrupt exit"
    );
    assert!(
        std::fs::metadata(path.with_extension("db-wal"))?.len() > 32,
        "fixture did not spill uncommitted WAL pages"
    );
    let mut reopened = LedgerStore::open_existing(&path, limits)?;
    let snapshot = reopened.load()?;
    assert_eq!(snapshot.revision, 1);
    assert_eq!(
        snapshot.ledger.to_json()?,
        original,
        "uncommitted bytes became visible"
    );

    // SQLite's per-connection page ceiling deterministically injects SQLITE_FULL into a save.
    let pages: u32 = reopened
        .connection
        .query_row("PRAGMA page_count", [], |r| r.get(0))?;
    reopened
        .connection
        .pragma_update(None, "max_page_count", pages)?;
    let mut next = snapshot.ledger;
    let mut fields = next
        .version(&VersionId::new("acct:witness-1:v1")?)
        .ok_or("fixture witness missing")?
        .fields()
        .clone();
    fields.id = OpaqueId::new("acct:additional")?;
    fields.version = VersionId::new("acct:additional:v1")?;
    fields.attributes.insert(
        AttributeName::new("large_text")?,
        FieldValue::Known(AttributeValue::Text("x".repeat(8192))),
    );
    next.issue(fields)?;
    assert!(matches!(
        reopened.append_json(1, &next.to_json()?),
        Err(StoreError::Database(rusqlite::Error::SqliteFailure(error, _)))
            if error.code == ErrorCode::DiskFull
    ));
    assert_eq!(reopened.load()?.ledger.to_json()?, original);
    drop(reopened);
    let mut recovered = LedgerStore::open_existing(&path, limits)?;
    assert_eq!(recovered.load()?.revision, 1);
    assert_eq!(recovered.load()?.ledger.to_json()?, original);
    Ok(())
}

#[test]
fn incompatible_corrupt_or_over_budget_store_is_not_repaired() -> TestResult {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("compatibility.db");
    let limits = StoreLimits::default();
    assert!(LedgerStore::open_existing(&path, limits).is_err());
    assert!(!path.exists(), "inspection created a missing store");
    let connection = Connection::open(&path)?;
    connection.execute_batch("CREATE TABLE unrelated(value TEXT);")?;
    drop(connection);
    let bytes = std::fs::read(&path)?;
    assert!(matches!(
        LedgerStore::open(&path, limits),
        Err(StoreError::ForeignDatabase { .. })
    ));
    assert_eq!(std::fs::read(&path)?, bytes);

    let path = directory.path().join("ledger.db");
    let mut store = LedgerStore::open(&path, limits)?;
    store.append_json(0, &fixture())?;
    store.connection.pragma_update(None, "user_version", 2)?;
    drop(store);
    let bytes = std::fs::read(&path)?;
    assert!(matches!(
        LedgerStore::open(&path, limits),
        Err(StoreError::UnsupportedSchema {
            found: 2,
            supported: 1
        })
    ));
    assert_eq!(std::fs::read(&path)?, bytes, "future schema was modified");
    let connection = Connection::open(&path)?;
    connection.pragma_update(None, "user_version", 1)?;
    drop(connection);
    let smaller = StoreLimits {
        max_ledger_bytes: 100,
        ..limits
    };
    assert!(matches!(
        LedgerStore::open_existing(&path, smaller),
        Err(StoreError::TooLarge { .. }) | Err(StoreError::Database(_))
    ));
    let connection = Connection::open(&path)?;
    connection.execute(
        "UPDATE ledger_snapshot SET payload = ?1",
        [b"{}".as_slice()],
    )?;
    drop(connection);
    assert!(matches!(
        LedgerStore::open_existing(&path, limits),
        Err(StoreError::InvalidLedger(_))
    ));
    let connection = Connection::open(&path)?;
    let payload: Vec<u8> =
        connection.query_row("SELECT payload FROM ledger_snapshot", [], |r| r.get(0))?;
    assert_eq!(
        payload, b"{}",
        "malformed persisted data was silently replaced"
    );
    Ok(())
}
