//! Tauri adapter. Rust owns file snapshots, operations, cancellation and publication,
//! plus the globe and witness instruments' state.
use crate::globe::{
    CameraView, GlobeState, GlobeView,
    imagery::{CivilDate, ImagerySelection},
    layers::AlprFilterId,
};
use crate::local_tools::{
    self as tools, CitationView, Control, CorpusSearchView, CorpusView, DocumentView, FileKind,
    ForensicView, PreparedExport, PrivacyView, Result, SNAPSHOT_SCHEME, SearchView, Selection,
    Snapshot, ToolError,
};
use crate::witness::{WitnessFormData, WitnessOrigin, WitnessState, WitnessView};
use serde::Serialize;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{Manager, State};

#[derive(Debug, Default)]
struct Inner {
    active: Option<Control>,
    document: Option<Snapshot>,
    image: Option<Snapshot>,
    /// The explicitly selected corpus, in selection order. Slots are not source identities.
    corpus: Vec<Snapshot>,
    /// Session-only citation records in creation order, valid and invalid alike.
    citations: Vec<CitationView>,
}
#[derive(Clone, Debug, Default)]
struct Services(Arc<Mutex<Inner>>);
impl Services {
    fn lock(&self) -> Result<MutexGuard<'_, Inner>> {
        self.0
            .lock()
            .map_err(|_| ToolError::new("internal", "The workspace lock failed. Restart OIA."))
    }
    fn begin(&self) -> Result<Operation> {
        let mut state = self.lock()?;
        if state.active.is_some() {
            return Err(ToolError::new(
                "busy",
                "Wait for the current operation to finish.",
            ));
        }
        let control = Control::default();
        state.active = Some(control.clone());
        Ok(Operation {
            services: self.clone(),
            control,
        })
    }
}
struct Operation {
    services: Services,
    control: Control,
}
impl Operation {
    fn snapshot(&self, kind: FileKind, sha256: &str) -> Result<Snapshot> {
        let state = self.services.lock()?;
        let file = match kind {
            FileKind::Text => &state.document,
            FileKind::Image => &state.image,
        };
        file.as_ref()
            .filter(|s| s.info.sha256 == sha256)
            .cloned()
            .ok_or_else(|| {
                ToolError::new(
                    "stale_input",
                    "The selected file changed. Open it and try again.",
                )
            })
    }
    /// A held text snapshot by hash: the reader's document or a corpus slot. Identical
    /// bytes in both are the same snapshot for citation purposes.
    fn text_snapshot(&self, sha256: &str) -> Result<Snapshot> {
        let state = self.services.lock()?;
        state
            .document
            .iter()
            .chain(state.corpus.iter())
            .find(|s| s.info.sha256 == sha256)
            .cloned()
            .ok_or_else(|| {
                ToolError::new(
                    "stale_input",
                    "That file is no longer held. Open it and try again.",
                )
            })
    }
    fn corpus(&self) -> Result<Vec<Snapshot>> {
        let corpus = self.services.lock()?.corpus.clone();
        if corpus.is_empty() {
            return Err(ToolError::new("no_corpus", "Open a corpus first."));
        }
        Ok(corpus)
    }
    fn publish<T>(&self, f: impl FnOnce(&mut Inner) -> Result<T>) -> Result<T> {
        // Cancellation and the final mutation share this gate; a cancelled attempt cannot publish.
        let mut state = self.services.lock()?;
        self.control.check()?;
        f(&mut state)
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        if let Ok(mut state) = self.services.lock() {
            state.active = None;
        }
    }
}

#[derive(Debug, Serialize)]
struct Envelope<T: Serialize> {
    schema: &'static str,
    data: T,
}
fn envelope<T: Serialize>(data: T) -> Envelope<T> {
    Envelope {
        schema: "oia-local-tools/1",
        data,
    }
}

fn choose_file(kind: FileKind, window: &tauri::WebviewWindow) -> Option<std::path::PathBuf> {
    match kind {
        FileKind::Text => rfd::FileDialog::new()
            .set_parent(window)
            .set_title("Open a UTF-8 document")
            .add_filter("Text documents", &["txt", "md", "csv", "json", "log"])
            .add_filter("All files", &["*"])
            .pick_file(),
        FileKind::Image => rfd::FileDialog::new()
            .set_parent(window)
            .set_title("Inspect image metadata")
            .add_filter("PNG or JPEG", &["png", "jpg", "jpeg"])
            .pick_file(),
    }
}

fn choose_corpus(window: &tauri::WebviewWindow) -> Option<Vec<std::path::PathBuf>> {
    rfd::FileDialog::new()
        .set_parent(window)
        .set_title(format!(
            "Open a corpus of UTF-8 documents (up to {} files)",
            tools::CORPUS_FILES
        ))
        .add_filter("Text documents", &["txt", "md", "csv", "json", "log"])
        .add_filter("All files", &["*"])
        .pick_files()
}

async fn worker<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| ToolError::new("worker", "The operation worker stopped unexpectedly."))?
}

#[tauri::command]
async fn open_document(
    state: State<'_, Services>,
    window: tauri::WebviewWindow,
) -> Result<Envelope<Option<DocumentView>>> {
    let mut operation = state.begin()?;
    worker(move || {
        let Some(path) = choose_file(FileKind::Text, &window) else {
            return Ok(envelope(None));
        };
        operation.control = operation.control.processing();
        let snapshot = tools::open_file(&path, FileKind::Text, &operation.control)?;
        let view = tools::document(&snapshot, &operation.control)?;
        operation.publish(|s| {
            s.document = Some(snapshot);
            Ok(envelope(Some(view)))
        })
    })
    .await
}

#[tauri::command]
async fn open_image(
    state: State<'_, Services>,
    window: tauri::WebviewWindow,
) -> Result<Envelope<Option<PrivacyView>>> {
    let mut operation = state.begin()?;
    worker(move || {
        let Some(path) = choose_file(FileKind::Image, &window) else {
            return Ok(envelope(None));
        };
        operation.control = operation.control.processing();
        let snapshot = tools::open_file(&path, FileKind::Image, &operation.control)?;
        let view = tools::inspect_image(&snapshot, &operation.control)?;
        operation.publish(|s| {
            s.image = Some(snapshot);
            Ok(envelope(Some(view)))
        })
    })
    .await
}

#[tauri::command]
async fn search_document(
    state: State<'_, Services>,
    query: String,
    sha256: String,
) -> Result<Envelope<SearchView>> {
    let operation = state.begin()?;
    worker(move || {
        let snapshot = operation.snapshot(FileKind::Text, &sha256)?;
        let result = tools::search(&snapshot, &query, &operation.control)?;
        operation.publish(|_| Ok(envelope(result)))
    })
    .await
}

/// Opens an explicitly selected corpus. A refused or cancelled selection keeps the previous
/// corpus; a loaded one replaces it. The reader's document and the citations are unchanged.
#[tauri::command]
async fn open_corpus(
    state: State<'_, Services>,
    window: tauri::WebviewWindow,
) -> Result<Envelope<Option<CorpusView>>> {
    let mut operation = state.begin()?;
    worker(move || {
        let Some(paths) = choose_corpus(&window) else {
            return Ok(envelope(None));
        };
        operation.control = operation.control.processing();
        let snapshots = tools::open_corpus(&paths, &operation.control)?;
        let view = tools::corpus_view(&snapshots);
        operation.publish(|s| {
            s.corpus = snapshots;
            Ok(envelope(Some(view)))
        })
    })
    .await
}

#[tauri::command]
async fn search_corpus(
    state: State<'_, Services>,
    query: String,
) -> Result<Envelope<CorpusSearchView>> {
    let mut operation = state.begin()?;
    worker(move || {
        let corpus = operation.corpus()?;
        operation.control = operation.control.processing();
        let result = tools::search_corpus(&corpus, &query, &operation.control)?;
        operation.publish(|_| Ok(envelope(result)))
    })
    .await
}

/// Opens a held corpus slot in the reader without a dialog. The snapshot is the same bytes
/// the corpus search anchored, so a match's offsets stay valid in the reader.
#[tauri::command]
async fn open_corpus_document(
    state: State<'_, Services>,
    sha256: String,
) -> Result<Envelope<DocumentView>> {
    let mut operation = state.begin()?;
    worker(move || {
        let snapshot = {
            let state = operation.services.lock()?;
            state
                .corpus
                .iter()
                .find(|s| s.info.sha256 == sha256)
                .cloned()
                .ok_or_else(|| {
                    ToolError::new("stale_input", "That file is not in the held corpus.")
                })?
        };
        operation.control = operation.control.processing();
        let view = tools::document(&snapshot, &operation.control)?;
        operation.publish(|s| {
            s.document = Some(snapshot);
            Ok(envelope(view))
        })
    })
    .await
}

#[derive(Debug, Serialize)]
struct CitationsView {
    records: Vec<CitationView>,
    limit: usize,
}
fn citations_view(state: &Inner) -> CitationsView {
    CitationsView {
        records: state.citations.clone(),
        limit: tools::CITATIONS,
    }
}
fn append_citation(state: &mut Inner, record: CitationView) -> Result<Envelope<CitationsView>> {
    if state.citations.len() >= tools::CITATIONS {
        return Err(ToolError::new(
            "citation_limit",
            format!(
                "This session holds {} citation records, the limit. The new record was not added.",
                tools::CITATIONS
            ),
        ));
    }
    state.citations.push(record);
    Ok(envelope(citations_view(state)))
}

#[tauri::command]
fn citations(state: State<'_, Services>) -> Result<Envelope<CitationsView>> {
    let state = state.lock()?;
    Ok(envelope(citations_view(&state)))
}

#[tauri::command]
async fn cite_selection(
    state: State<'_, Services>,
    sha256: String,
    start: usize,
    end: usize,
) -> Result<Envelope<CitationsView>> {
    let mut operation = state.begin()?;
    worker(move || {
        let snapshot = operation.text_snapshot(&sha256)?;
        operation.control = operation.control.processing();
        let record = tools::cite_selection(&snapshot, start, end, &operation.control)?;
        operation.publish(|s| append_citation(s, record))
    })
    .await
}

#[tauri::command]
async fn check_citation(
    state: State<'_, Services>,
    sha256: String,
    start: usize,
    end: usize,
    quote: String,
) -> Result<Envelope<CitationsView>> {
    let mut operation = state.begin()?;
    worker(move || {
        let snapshot = operation.text_snapshot(&sha256)?;
        operation.control = operation.control.processing();
        let record = tools::check_citation(&snapshot, start, end, &quote, &operation.control)?;
        operation.publish(|s| append_citation(s, record))
    })
    .await
}

#[derive(Debug, Serialize)]
struct ExportView {
    path: String,
    removed: usize,
    bytes: usize,
    sha256: String,
}

#[tauri::command]
async fn export_cleaned(
    state: State<'_, Services>,
    window: tauri::WebviewWindow,
    sha256: String,
    selection: Selection,
) -> Result<Envelope<Option<ExportView>>> {
    let mut operation = state.begin()?;
    worker(move || {
        let snapshot = operation.snapshot(FileKind::Image, &sha256)?;
        let name = std::path::Path::new(&snapshot.info.name);
        let stem = name.file_stem().unwrap_or_default().to_string_lossy();
        let extension = if snapshot.bytes().starts_with(b"\x89PNG") {
            "png"
        } else {
            "jpg"
        };
        let Some(destination) = rfd::FileDialog::new()
            .set_parent(&window)
            .set_title("Save a cleaned copy — choose a new filename")
            .set_file_name(format!("{stem}-cleaned.{extension}"))
            .save_file()
        else {
            return Ok(envelope(None));
        };
        operation.control = operation.control.processing();
        let cleaned = tools::clean_image(&snapshot, &selection, &operation.control)?;
        use sha2::{Digest, Sha256};
        let sha256 = format!("{:x}", Sha256::digest(&cleaned.bytes));
        let staged = PreparedExport::prepare(&destination, &cleaned.bytes, &operation.control)?;
        operation.publish(|_| {
            let path = staged.commit(&operation.control)?;
            Ok(envelope(Some(ExportView {
                path: path.to_string_lossy().into_owned(),
                removed: cleaned.removed,
                bytes: cleaned.bytes.len(),
                sha256,
            })))
        })
    })
    .await
}

#[tauri::command]
async fn analyze_image(
    state: State<'_, Services>,
    sha256: String,
) -> Result<Envelope<ForensicView>> {
    let mut operation = state.begin()?;
    worker(move || {
        let snapshot = operation.snapshot(FileKind::Image, &sha256)?;
        operation.control = operation.control.processing();
        let view = tools::analyze_image(&snapshot, &operation.control)?;
        operation.publish(|_| Ok(envelope(view)))
    })
    .await
}

#[derive(Debug, Serialize)]
struct ResidualExportView {
    path: String,
    bytes: usize,
    sha256: String,
    source_sha256: String,
}

#[tauri::command]
async fn export_residual(
    state: State<'_, Services>,
    window: tauri::WebviewWindow,
    sha256: String,
) -> Result<Envelope<Option<ResidualExportView>>> {
    let mut operation = state.begin()?;
    worker(move || {
        let snapshot = operation.snapshot(FileKind::Image, &sha256)?;
        if tools::media_type(snapshot.bytes()) != Some("image/jpeg") {
            return Err(ToolError::new(
                "forensic_unsupported",
                "Residual maps require a JPEG image.",
            ));
        }
        let stem = std::path::Path::new(&snapshot.info.name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let Some(destination) = rfd::FileDialog::new()
            .set_parent(&window)
            .set_title("Save a diagnostic residual map — choose a new filename")
            .set_file_name(format!("{stem}-residual-diagnostic.png"))
            .add_filter("Diagnostic PNG", &["png"])
            .save_file()
        else {
            return Ok(envelope(None));
        };
        operation.control = operation.control.processing();
        let bytes = tools::residual_map(&snapshot, &operation.control)?;
        use sha2::{Digest, Sha256};
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        let staged = PreparedExport::prepare(&destination, &bytes, &operation.control)?;
        operation.publish(|_| {
            let path = staged.commit(&operation.control)?;
            Ok(envelope(Some(ResidualExportView {
                path: path.to_string_lossy().into_owned(),
                bytes: bytes.len(),
                sha256,
                source_sha256: snapshot.info.sha256,
            })))
        })
    })
    .await
}

/// Serves the held image snapshot's original bytes to the page, read-only, keyed by its
/// SHA-256. Anything else under the scheme is a 404. No path, file or other state is
/// reachable through it, and nothing is cached. The response allows cross-origin reads
/// because the page's own origin differs from the scheme's: without it the WebView ignores
/// `image-orientation: none` and would display an EXIF-rotated image in a different
/// orientation from the forensic report's encoded raster coordinates.
fn snapshot_protocol(
    ctx: tauri::UriSchemeContext<'_, tauri::Wry>,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let respond = |status: u16, media_type: &str, body: Vec<u8>| {
        tauri::http::Response::builder()
            .status(status)
            .header("Content-Type", media_type)
            .header("Cache-Control", "no-store")
            .header("X-Content-Type-Options", "nosniff")
            .header("Access-Control-Allow-Origin", "*")
            .body(body)
            .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
    };
    let path = request.uri().path().trim_start_matches('/');
    let valid = path.len() == 64 && path.bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        return respond(404, "text/plain", b"not found".to_vec());
    }
    let services = ctx.app_handle().state::<Services>();
    let Ok(state) = services.lock() else {
        return respond(500, "text/plain", b"lock failed".to_vec());
    };
    match state
        .image
        .as_ref()
        .filter(|s| s.info.sha256 == path)
        .map(|s| (tools::media_type(s.bytes()), s.bytes().to_vec()))
    {
        Some((Some(media_type), bytes)) => respond(200, media_type, bytes),
        _ => respond(404, "text/plain", b"not found".to_vec()),
    }
}

#[tauri::command]
fn cancel_operation(state: State<'_, Services>) -> Result<()> {
    if let Some(control) = &state.lock()?.active {
        control.cancel();
    }
    Ok(())
}

/// Globe instrument state. Independent of the file-operation gate: a camera move or a
/// selection never waits on a running document or image operation, and never mutates one.
#[derive(Clone, Debug, Default)]
struct Globe(Arc<Mutex<GlobeState>>);
impl Globe {
    fn lock(&self) -> Result<MutexGuard<'_, GlobeState>> {
        self.0
            .lock()
            .map_err(|_| ToolError::new("internal", "The globe lock failed. Restart OIA."))
    }
}

#[tauri::command]
fn globe_view(globe: State<'_, Globe>) -> Result<Envelope<GlobeView>> {
    globe.lock()?.view().map(envelope)
}

#[tauri::command]
fn globe_select(globe: State<'_, Globe>, record_id: Option<String>) -> Result<Envelope<GlobeView>> {
    let mut state = globe.lock()?;
    state.select(record_id)?;
    state.view().map(envelope)
}

#[tauri::command]
fn globe_camera(globe: State<'_, Globe>, camera: CameraView) -> Result<Envelope<GlobeView>> {
    let mut state = globe.lock()?;
    state.set_camera(camera)?;
    state.view().map(envelope)
}

#[tauri::command]
fn globe_imagery(
    globe: State<'_, Globe>,
    selection: ImagerySelection,
) -> Result<Envelope<GlobeView>> {
    let today = CivilDate::today_utc(); // the one place the clock is read for the globe
    let mut state = globe.lock()?;
    state.set_imagery(selection, today)?;
    state.view().map(envelope)
}

#[tauri::command]
fn globe_layer(
    globe: State<'_, Globe>,
    layer_id: String,
    visible: bool,
) -> Result<Envelope<GlobeView>> {
    let mut state = globe.lock()?;
    state.set_layer_visible(&layer_id, visible)?;
    state.view().map(envelope)
}

#[tauri::command]
fn globe_alpr_filter(globe: State<'_, Globe>, filter: AlprFilterId) -> Result<Envelope<GlobeView>> {
    let mut state = globe.lock()?;
    state.set_alpr_filter(filter)?;
    state.view().map(envelope)
}

/// Witness capture is independent of file work and globe rendering. Its transitions are
/// synchronous, bounded by the domain kernel, and never published as durable evidence.
#[derive(Clone, Debug, Default)]
struct Witness(Arc<Mutex<WitnessState>>);
impl Witness {
    fn lock(&self) -> Result<MutexGuard<'_, WitnessState>> {
        self.0
            .lock()
            .map_err(|_| ToolError::new("internal", "The witness lock failed. Restart OIA."))
    }
}

#[tauri::command]
fn witness_view(witness: State<'_, Witness>) -> Result<Envelope<WitnessView>> {
    Ok(envelope(witness.lock()?.view()))
}

#[tauri::command]
fn witness_capture(
    witness: State<'_, Witness>,
    origin: WitnessOrigin,
    form: WitnessFormData,
) -> Result<Envelope<WitnessView>> {
    let view = witness.lock()?.capture_initial(origin, form)?;
    Ok(envelope(view))
}

#[tauri::command]
fn witness_revise(
    witness: State<'_, Witness>,
    expected_version: String,
    form: WitnessFormData,
    rationale: String,
) -> Result<Envelope<WitnessView>> {
    let view = witness.lock()?.revise(&expected_version, form, rationale)?;
    Ok(envelope(view))
}

#[tauri::command]
fn witness_expose_explanations(
    witness: State<'_, Witness>,
    expected_version: String,
) -> Result<Envelope<WitnessView>> {
    let view = witness.lock()?.expose_explanations(&expected_version)?;
    Ok(envelope(view))
}

pub fn run() -> std::result::Result<(), tauri::Error> {
    tauri::Builder::default()
        .manage(Services::default())
        .manage(Globe::default())
        .register_uri_scheme_protocol(SNAPSHOT_SCHEME, snapshot_protocol)
        .manage(Witness::default())
        .invoke_handler(tauri::generate_handler![
            open_document,
            open_image,
            search_document,
            open_corpus,
            search_corpus,
            open_corpus_document,
            citations,
            cite_selection,
            check_citation,
            export_cleaned,
            analyze_image,
            export_residual,
            cancel_operation,
            globe_view,
            globe_select,
            globe_camera,
            globe_imagery,
            globe_layer,
            globe_alpr_filter,
            witness_view,
            witness_capture,
            witness_revise,
            witness_expose_explanations
        ])
        .run(tauri::generate_context!())
}

#[cfg(test)]
mod tests {
    use super::*;

    // CASE-04: cancellation must fence publication, and a second worker cannot overlap.
    #[test]
    fn cancelled_operation_cannot_publish_or_overlap() -> Result<()> {
        let services = Services::default();
        let operation = services.begin()?;
        assert!(services.begin().is_err());
        if let Some(control) = &services.lock()?.active {
            control.cancel();
        }
        let mut published = false;
        assert!(
            operation
                .publish(|_| {
                    published = true;
                    Ok(())
                })
                .is_err()
        );
        assert!(!published);
        drop(operation);
        assert!(services.begin().is_ok());
        Ok(())
    }
}
