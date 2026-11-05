//! Local persist/reopen tool for the canonical domain JSON; no network or device operations.
use std::error::Error;
use std::fs::File;
use std::io::Read;

use oia_store::{LedgerStore, StoreLimits};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let limits = StoreLimits::default();
    match args.as_slice() {
        [command, database] if command == "inspect" => {
            let mut store = LedgerStore::open_existing(database, limits)?;
            let snapshot = store.load()?;
            eprintln!(
                "revision={} entries={} max_ledger_bytes={}",
                snapshot.revision,
                snapshot.ledger.entries().len(),
                limits.max_ledger_bytes
            );
            print!("{}", snapshot.ledger.to_json()?);
        }
        [command, database, revision, input] if command == "append" => {
            let expected: u64 = revision
                .to_str()
                .ok_or("revision must be UTF-8 digits")?
                .parse()?;
            // Read at most one byte past the budget, even if the file grows while being read.
            let mut text = String::new();
            File::open(input)?
                .take(u64::try_from(limits.max_ledger_bytes)? + 1)
                .read_to_string(&mut text)?;
            if text.len() > limits.max_ledger_bytes {
                return Err("ledger input exceeds the 8 MiB example budget".into());
            }
            // Validate before creating a new store, so a malformed input creates no case state.
            oia_domain::evidence::EvidenceLedger::from_json(&text, limits.ledger)?;
            let mut store = LedgerStore::open(database, limits)?;
            let saved = store.append_json(expected, &text)?;
            println!(
                "saved revision={saved} max_ledger_bytes={}",
                limits.max_ledger_bytes
            );
        }
        _ => return Err(
            "usage: ledger-store inspect DATABASE | append DATABASE EXPECTED_REVISION LEDGER_JSON"
                .into(),
        ),
    }
    Ok(())
}
