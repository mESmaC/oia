//! Application adapter over the one shared source registry. No WebView network authority.
use crate::local_tools::{Control, Result, ToolError};
use oia_ingest::{
    collection::{Category, Error, Registry, View, now_ms},
    transport::HttpTransport,
};
use serde::Serialize;
use std::sync::{Mutex, OnceLock};

static HTTP: OnceLock<std::result::Result<Mutex<HttpTransport>, Error>> = OnceLock::new();

pub fn tool_error(error: Error) -> ToolError {
    ToolError::new(error.code, error.message)
}
pub fn view(registry: &Registry, category: Category) -> Result<View> {
    Ok(registry.view(category, now_ms().map_err(tool_error)?))
}
pub fn refresh(
    registry: &mut Registry,
    category: Category,
    query: &str,
    control: &Control,
) -> Result<View> {
    let mut transport = HTTP
        .get_or_init(|| HttpTransport::new().map(Mutex::new))
        .as_ref()
        .map_err(|error| tool_error(error.clone()))?
        .try_lock()
        .map_err(|_| ToolError::new("busy", "A source request is already running."))?;
    registry
        .refresh(category, query, &mut *transport, &|| {
            control
                .check()
                .map_err(|error| Error::new(error.code, error.message))
        })
        .map_err(tool_error)?;
    control.check()?;
    view(registry, category)
}

#[derive(Debug, Serialize)]
pub struct OriginalView {
    pub source_id: String,
    pub sha256: String,
    pub request_url: String,
    pub text: String,
    pub evidence_ledger: String,
}
pub fn original(
    registry: &Registry,
    source_id: &str,
    sha256: &str,
    request_url: &str,
) -> Result<OriginalView> {
    let snapshot = registry
        .snapshot(source_id, sha256, request_url)
        .map_err(tool_error)?;
    let text = std::str::from_utf8(snapshot.original())
        .map_err(|_| ToolError::new("encoding", "This retained response is not UTF-8."))?
        .to_owned();
    Ok(OriginalView {
        source_id: source_id.into(),
        sha256: sha256.into(),
        request_url: request_url.into(),
        text,
        evidence_ledger: snapshot
            .evidence()
            .to_json()
            .map_err(|error| ToolError::new("evidence", error))?,
    })
}
