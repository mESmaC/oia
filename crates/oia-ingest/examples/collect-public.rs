//! Live smoke probe of the same registry/transport used by the desktop. Network required.
use oia_ingest::{
    collection::{Category, Registry, now_ms},
    transport::HttpTransport,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = Registry::default();
    let mut transport = HttpTransport::new()?;
    for category in [Category::News, Category::Reports] {
        registry.refresh(category, "", &mut transport, &|| Ok(()))?;
        println!(
            "{}",
            serde_json::to_string_pretty(&registry.view(category, now_ms()?))?
        );
    }
    Ok(())
}
