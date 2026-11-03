#[path = "support/recreation_fixture.rs"]
mod recreation_fixture;

use oia_app::recreation_composer::compose_recreation_prompt;
use oia_domain::{OpaqueId, VersionId};
use oia_media::recreation::AssetExportPolicy;
use recreation_fixture::{Fixture, limits};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let sources = fixture.sources()?;
    let first = compose_recreation_prompt(
        fixture.request(&sources, "prompt-version:1")?,
        None,
        limits()?,
        AssetExportPolicy::OmitWithNotice,
        || Ok(()),
    )?;
    println!("{}", first.portable_text());
    let mut expanded = sources.clone();
    let mut late = sources.get(2).ok_or("missing fixture witness")?.clone();
    late.account = OpaqueId::new("account:late")?;
    late.version = VersionId::new("version:late-1")?;
    expanded.push(late);
    let second = compose_recreation_prompt(
        fixture.request(&expanded, "prompt-version:2")?,
        Some(&first),
        limits()?,
        AssetExportPolicy::OmitWithNotice,
        || Ok(()),
    )?;
    println!("\nRegenerated version changes: {:?}", second.changes());
    println!(
        "Earlier version retained: {}",
        first.exported().prompt_version()
    );
    Ok(())
}
