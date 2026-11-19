//! Explicit, session-only Caltrans highway still browsing (GEO-02/04, CAM-01/02).
//! No pose, capture timestamp, archive, scene absence or evidentiary freshness is inferred.
use crate::local_tools::{Control, Result, ToolError};
use oia_ingest::{
    collection::Error as IngestError,
    transport::{HttpPolicy, HttpTransport, Request},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const LAYER_ID: &str = "traffic-cameras";
pub const ORIGIN: &str = "https://cwwp2.dot.ca.gov";
pub const CATALOG_LIMIT: usize = 4 * 1024 * 1024;
pub const IMAGE_LIMIT: usize = 2 * 1024 * 1024;
pub const CAMERA_LIMIT: usize = 2_000;
pub const STATEMENT: &str = "Caltrans highway views, separate from the historical case and satellite/aerial mosaics. Capture time and freshness are unknown; record timestamps describe catalog edits. No calibrated pose, footprint, archive or event-evidence eligibility. Failed acquisition is a coverage gap, never an empty scene. No recording or automatic polling.";

// Separate bounded camera reservations, using the same ingest transport implementation
// as public collection. The provider-specific admission rules remain below.
static HTTP: OnceLock<std::result::Result<Mutex<HttpTransport>, IngestError>> = OnceLock::new();

fn catalog_url(district: u8) -> String {
    format!("{ORIGIN}/data/d{district}/cctv/cctvStatusD{district:02}.json")
}

fn error(message: impl ToString) -> ToolError {
    ToolError::new("traffic_source", message)
}
fn now_ms() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|t| u64::try_from(t.as_millis()).ok())
        .ok_or_else(|| error("The system clock cannot supply a UTC retrieval time."))
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Camera {
    pub id: String,
    pub label: String,
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub image_url: String,
    pub record_date: Option<String>,
    pub record_time: Option<String>,
    pub direction_text: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Catalog {
    pub district: u8,
    pub cameras: Vec<Camera>,
    pub total: usize,
    pub out_of_service: usize,
    pub invalid: usize,
    pub sha256: String,
    pub retrieved_at_ms: u64,
    pub original: Arc<Vec<u8>>,
}

fn text<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value
        .get(key)?
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 512 && !s.chars().any(char::is_control))
}

/// Accept only a literal HTTPS URL under this district's published still directory.
/// No credentials, query, redirect, alternate host, previous-frame path or encoded path.
fn image_url(value: &str, district: u8) -> bool {
    let prefix = format!("{ORIGIN}/data/d{district}/cctv/image/");
    let Some(tail) = value.strip_prefix(&prefix) else {
        return false;
    };
    let parts: Vec<_> = tail.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|s| {
            !s.is_empty()
                && !s.contains("..")
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-./".contains(&b))
        })
        && tail.ends_with(".jpg")
}

pub fn parse_catalog(bytes: Vec<u8>, district: u8, retrieved_at_ms: u64) -> Result<Catalog> {
    if !(1..=12).contains(&district) || bytes.len() > CATALOG_LIMIT {
        return Err(error("Catalog district or byte limit is invalid."));
    }
    let json: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| error("Caltrans did not return a valid JSON catalog."))?;
    let rows = json
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| error("Caltrans catalog has no data array."))?;
    if rows.len() > CAMERA_LIMIT {
        return Err(error(
            "Catalog exceeds the 2,000-record limit; no replacement applied.",
        ));
    }
    let mut result = Catalog {
        district,
        cameras: Vec::new(),
        total: rows.len(),
        out_of_service: 0,
        invalid: 0,
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        retrieved_at_ms,
        original: Arc::new(bytes.clone()),
    };
    let mut ids = BTreeSet::new();
    for row in rows {
        let c = &row["cctv"];
        match text(c, "inService") {
            Some("false") => {
                result.out_of_service += 1;
                continue;
            }
            Some("true") => {}
            _ => {
                result.invalid += 1;
                continue;
            }
        }
        let loc = &c["location"];
        let parsed = (|| {
            if text(loc, "district")?.parse::<u8>().ok()? != district {
                return None;
            }
            let index = text(c, "index")?;
            if index.len() > 16 || !index.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let id = format!("traffic:{district}:{index}");
            let lat_deg = text(loc, "latitude")?.parse::<f64>().ok()?;
            let lon_deg = text(loc, "longitude")?.parse::<f64>().ok()?;
            // California highway bounds also reject placeholder zero coordinates.
            if !lat_deg.is_finite()
                || !lon_deg.is_finite()
                || !(32.0..=42.1).contains(&lat_deg)
                || !(-124.6..=-114.0).contains(&lon_deg)
            {
                return None;
            }
            let url = text(&c["imageData"]["static"], "currentImageURL")?;
            if !image_url(url, district) {
                return None;
            }
            Some(Camera {
                id,
                label: text(loc, "locationName")?.to_owned(),
                lat_deg,
                lon_deg,
                image_url: url.to_owned(),
                record_date: text(&c["recordTimestamp"], "recordDate").map(str::to_owned),
                record_time: text(&c["recordTimestamp"], "recordTime").map(str::to_owned),
                direction_text: text(loc, "direction").map(str::to_owned),
            })
        })();
        if let Some(camera) = parsed.filter(|camera| ids.insert(camera.id.clone())) {
            result.cameras.push(camera);
        } else {
            result.invalid += 1;
        }
    }
    Ok(result)
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FrameView {
    pub camera_id: String,
    pub image_path: String,
    pub sha256: String,
    pub catalog_sha256: String,
    pub source_url: String,
    pub retrieved_at_ms: u64,
    pub bytes: usize,
    pub width: u32,
    pub height: u32,
    pub same_bytes_as_previous: bool,
    pub eligible_as_event_evidence: bool,
}
#[derive(Clone, Debug)]
pub struct Frame {
    pub view: FrameView,
    pub original: Arc<Vec<u8>>,
}

fn download(url: &str, limit: usize, accept: &'static str, control: &Control) -> Result<Vec<u8>> {
    control.check()?;
    let mut transport = HTTP
        .get_or_init(|| HttpTransport::new().map(Mutex::new))
        .as_ref()
        .map_err(|e| ToolError::new(e.code, &e.message))?
        .try_lock()
        .map_err(|_| error("A camera request is already finishing."))?;
    let response = transport
        .get_with_policy(
            Request {
                url,
                cadence_seconds: 15,
                etag: None,
                last_modified: None,
            },
            HttpPolicy {
                accept,
                expected_content_type: Some(accept),
                max_bytes: limit,
                deadline: Duration::from_secs(20),
                connect_timeout: Duration::from_secs(8),
                endpoint_budget: CAMERA_LIMIT + 12,
                no_cache: true,
            },
            &|| {
                control
                    .check()
                    .map_err(|e| IngestError::new(e.code, e.message))
            },
        )
        .map_err(|e| ToolError::new(e.code, e.message))?;
    let refusal = match response.status {
        401 => Some((
            "traffic_http_401",
            "Caltrans returned HTTP 401. This origin is disabled for this session; no retry or alternate access.",
        )),
        403 => Some((
            "traffic_http_403",
            "Caltrans returned HTTP 403. This origin is disabled for this session; no retry or alternate access.",
        )),
        404 => Some((
            "traffic_http_404",
            "Caltrans returned HTTP 404. This resource is unavailable and disabled for this session.",
        )),
        410 => Some((
            "traffic_http_410",
            "Caltrans returned HTTP 410. This resource is gone and disabled for this session.",
        )),
        429 => Some((
            "traffic_http_429",
            "Caltrans returned HTTP 429. The provider retry interval is enforced; no automatic retry.",
        )),
        _ => None,
    };
    if let Some((code, message)) = refusal {
        return Err(ToolError::new(code, message));
    }
    if response.status != 200 {
        return Err(error(format!(
            "Source returned HTTP {}; no substitute was used.",
            response.status
        )));
    }
    if response.no_store {
        return Err(error(
            "Source forbids storage; no catalog or still retained.",
        ));
    }
    control.check()?;
    Ok(response.body)
}

pub fn fetch_catalog(district: u8, control: &Control) -> Result<Catalog> {
    if !(1..=12).contains(&district) {
        return Err(error("Choose Caltrans district 1 through 12."));
    }
    let url = catalog_url(district);
    let bytes = download(&url, CATALOG_LIMIT, "application/json", control)?;
    let catalog = parse_catalog(bytes, district, now_ms()?)?;
    control.check()?;
    Ok(catalog)
}

pub fn fetch_frame(
    camera: &Camera,
    catalog_sha256: &str,
    district: u8,
    control: &Control,
) -> Result<Frame> {
    if !image_url(&camera.image_url, district) {
        return Err(error("Still URL is outside the registered district."));
    }
    let bytes = download(&camera.image_url, IMAGE_LIMIT, "image/jpeg", control)?;
    if !bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Err(error("Source did not return a JPEG still."));
    }
    let (width, height) =
        image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Jpeg)
            .into_dimensions()
            .map_err(|_| error("Still image dimensions are malformed."))?;
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || u64::from(width) * u64::from(height) > 8_000_000
    {
        return Err(error(
            "Still exceeds the 4,096-side / 8-million-pixel display budget.",
        ));
    }
    control.check()?;
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    Ok(Frame {
        view: FrameView {
            camera_id: camera.id.clone(),
            image_path: format!("traffic/{sha256}"),
            sha256,
            catalog_sha256: catalog_sha256.to_owned(),
            source_url: camera.image_url.clone(),
            retrieved_at_ms: now_ms()?,
            bytes: bytes.len(),
            width,
            height,
            same_bytes_as_previous: false,
            eligible_as_event_evidence: false,
        },
        original: Arc::new(bytes),
    })
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TrafficView {
    pub visible: bool,
    pub busy: bool,
    pub district: Option<u8>,
    pub cameras: Vec<Camera>,
    pub total: usize,
    pub out_of_service: usize,
    pub invalid: usize,
    pub catalog_sha256: Option<String>,
    pub catalog_bytes: usize,
    pub retrieved_at_ms: Option<u64>,
    pub frame: Option<FrameView>,
    pub error: Option<String>,
    pub error_code: Option<&'static str>,
    pub statement: &'static str,
    pub source_url: &'static str,
    pub terms_url: &'static str,
    pub attribution: &'static str,
}

#[derive(Clone, Debug, Default)]
pub struct TrafficState {
    pub visible: bool,
    catalog: Option<Catalog>,
    frame: Option<Frame>,
    selection: Option<String>,
    active: Option<(u64, Control)>,
    generation: u64,
    // Cancelled requests keep the slot until the bounded worker exits.
    occupied: bool,
    last_catalog_request: Option<Instant>,
    last_frame_request: Option<Instant>,
    blocked: BTreeSet<String>,
    origin_blocked: bool,
    error: Option<ToolError>,
}

impl TrafficState {
    pub fn view(&self) -> TrafficView {
        TrafficView {
            visible: self.visible,
            busy: self.occupied,
            district: self.catalog.as_ref().map(|c| c.district),
            cameras: self
                .catalog
                .as_ref()
                .map(|c| c.cameras.clone())
                .unwrap_or_default(),
            total: self.catalog.as_ref().map_or(0, |c| c.total),
            out_of_service: self.catalog.as_ref().map_or(0, |c| c.out_of_service),
            invalid: self.catalog.as_ref().map_or(0, |c| c.invalid),
            catalog_sha256: self.catalog.as_ref().map(|c| c.sha256.clone()),
            catalog_bytes: self.catalog.as_ref().map_or(0, |c| c.original.len()),
            retrieved_at_ms: self.catalog.as_ref().map(|c| c.retrieved_at_ms),
            frame: self.frame.as_ref().map(|f| f.view.clone()),
            error: self.error.as_ref().map(|e| e.message.clone()),
            error_code: self.error.as_ref().map(|e| e.code),
            statement: STATEMENT,
            source_url: "https://cwwp2.dot.ca.gov/documentation/cctv/cctv.htm",
            terms_url: "https://dot.ca.gov/conditions-of-use",
            attribution: "Caltrans — California Department of Transportation",
        }
    }
    pub fn layer(&self) -> super::layers::LayerView {
        let cameras = self
            .catalog
            .as_ref()
            .map(|c| c.cameras.as_slice())
            .unwrap_or_default();
        let extent = cameras.first().map(|first| {
            cameras.iter().fold(
                super::layers::LayerExtent {
                    south: first.lat_deg,
                    north: first.lat_deg,
                    west: first.lon_deg,
                    east: first.lon_deg,
                },
                |mut e, c| {
                    e.south = e.south.min(c.lat_deg);
                    e.north = e.north.max(c.lat_deg);
                    e.west = e.west.min(c.lon_deg);
                    e.east = e.east.max(c.lon_deg);
                    e
                },
            )
        });
        super::layers::LayerView {
            id: LAYER_ID,
            label: "Caltrans traffic stills",
            kind: "traffic",
            visible: self.visible,
            origin_class: "provider_catalog",
            requirement: "GEO-02 / CAM-01 / CAM-02",
            source_label: self.catalog.as_ref().map_or_else(
                || "Caltrans - choose a district".to_owned(),
                |c| format!("Caltrans district {}", c.district),
            ),
            fixture_path: None,
            statement: STATEMENT,
            attribution: "Caltrans - California Department of Transportation",
            license: "Caltrans Conditions of Use; third-party exceptions apply. No image redistribution in this slice.",
            extent,
            shown: if self.visible { cameras.len() } else { 0 },
            total: cameras.len(),
            coverage: if self.catalog.is_some() {
                "partial: one district catalog; gaps and omitted records remain".to_owned()
            } else {
                "not requested".to_owned()
            },
            error: None,
        }
    }
    pub fn selectable(&self, id: &str) -> bool {
        self.visible
            && self
                .catalog
                .as_ref()
                .is_some_and(|c| c.cameras.iter().any(|c| c.id == id))
    }
    pub fn select(&mut self, id: Option<&str>) {
        let next = id.filter(|s| s.starts_with("traffic:")).map(str::to_owned);
        if next != self.selection {
            self.cancel();
            self.frame = None;
            self.selection = next;
        }
    }
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if !visible {
            self.select(None);
            self.cancel();
        }
    }
    pub fn cancel(&mut self) {
        if let Some((_, control)) = self.active.take() {
            control.cancel();
        }
    }
    fn begin(&mut self, resource: &str, catalog: bool) -> Result<(u64, Control)> {
        if self.origin_blocked {
            return Err(error(
                "Caltrans access is disabled after HTTP 401/403. No retry or alternate access in this session.",
            ));
        }
        if self.blocked.len() >= CAMERA_LIMIT + 12 {
            return Err(error(
                "The session restriction-ledger budget is exhausted; no further camera requests.",
            ));
        }
        if self.blocked.contains(resource) {
            return Err(error(
                "Resource is disabled after a restricted/unavailable response. No retry or alternate access in this session.",
            ));
        }
        if self.occupied {
            return Err(error(
                "A camera request is still finishing; wait for its bounded worker.",
            ));
        }
        let last = if catalog {
            &mut self.last_catalog_request
        } else {
            &mut self.last_frame_request
        };
        if last.is_some_and(|t| t.elapsed() < Duration::from_secs(15)) {
            return Err(error(
                "Wait 15 seconds between catalog requests or between still requests.",
            ));
        }
        *last = Some(Instant::now());
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| error("Request generation exhausted; restart OIA."))?;
        let control = Control::default();
        self.active = Some((self.generation, control.clone()));
        self.occupied = true;
        self.error = None;
        Ok((self.generation, control))
    }
    pub fn begin_catalog(&mut self, district: u8) -> Result<(u64, Control)> {
        if !(1..=12).contains(&district) {
            return Err(error("Choose Caltrans district 1 through 12."));
        }
        self.begin(&catalog_url(district), true)
    }
    pub fn begin_frame(&mut self) -> Result<(u64, Control, Camera, String, u8)> {
        let catalog = self
            .catalog
            .as_ref()
            .ok_or_else(|| error("Load a district first."))?;
        if self.blocked.contains(&catalog_url(catalog.district)) {
            return Err(error(
                "This district is disabled after a restricted/unavailable response; its old camera listing cannot authorize further requests.",
            ));
        }
        let camera = catalog
            .cameras
            .iter()
            .find(|c| Some(&c.id) == self.selection.as_ref())
            .filter(|_| self.visible)
            .cloned()
            .ok_or_else(|| error("Select a camera from the visible traffic layer."))?;
        let sha256 = catalog.sha256.clone();
        let district = catalog.district;
        let (token, control) = self.begin(&camera.image_url, false)?;
        Ok((token, control, camera, sha256, district))
    }
    fn finish<T>(&mut self, token: u64, resource: &str, result: Result<T>) -> Option<T> {
        // Record access restrictions even for a cancelled completion, preventing retries.
        if let Err(e) = &result {
            match e.code {
                "traffic_http_401" | "traffic_http_403" => self.origin_blocked = true,
                "traffic_http_404" | "traffic_http_410" => {
                    self.blocked.insert(resource.to_owned());
                }
                _ => {}
            }
        }
        if token != self.generation {
            return None;
        }
        self.occupied = false;
        let (_, control) = self.active.take().filter(|(t, _)| *t == token)?;
        if let Err(e) = control.check() {
            if e.code != "cancelled" {
                self.error = Some(e);
            }
            return None;
        }
        match result {
            Ok(value) => Some(value),
            Err(e) => {
                self.error = Some(e);
                None
            }
        }
    }
    pub fn finish_catalog(&mut self, token: u64, district: u8, result: Result<Catalog>) -> bool {
        if let Some(catalog) = self.finish(token, &catalog_url(district), result) {
            self.catalog = Some(catalog);
            self.frame = None;
            self.selection = None;
            self.visible = true;
            return true;
        }
        false
    }
    pub fn finish_frame(&mut self, token: u64, resource_url: &str, result: Result<Frame>) {
        if let Some(mut frame) = self.finish(token, resource_url, result) {
            frame.view.same_bytes_as_previous = self
                .frame
                .as_ref()
                .is_some_and(|f| f.view.sha256 == frame.view.sha256);
            self.frame = Some(frame);
        }
    }
    pub fn image(&self, hash: &str) -> Option<Vec<u8>> {
        self.frame
            .as_ref()
            .filter(|f| {
                self.visible
                    && Some(&f.view.camera_id) == self.selection.as_ref()
                    && f.view.sha256 == hash
            })
            .map(|f| f.original.as_ref().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oia_ingest::transport::public_ip;
    #[test]
    fn catalog_preserves_bytes_and_refuses_substitutes() -> Result<()> {
        let row = serde_json::json!({"cctv":{"index":"1","inService":"true","location":{"district":"4","latitude":"37.8","longitude":"-122.2","locationName":"Highway view"},"imageData":{"static":{"currentImageURL":"https://cwwp2.dot.ca.gov/data/d4/cctv/image/cam/cam.jpg"}}}});
        let mut malicious = row.clone();
        malicious["cctv"]["index"] = "2".into();
        malicious["cctv"]["imageData"]["static"]["currentImageURL"] =
            "https://example.com/a.jpg".into();
        let mut absent = row.clone();
        absent["cctv"]["index"] = "3".into();
        absent["cctv"]["location"]["latitude"] = "0".into();
        let bytes = serde_json::to_vec(&serde_json::json!({"data":[row,malicious,absent]}))
            .map_err(error)?;
        let c = parse_catalog(bytes.clone(), 4, 1)?;
        assert_eq!((c.cameras.len(), c.invalid, c.total), (1, 2, 3));
        assert_eq!(*c.original, bytes);
        for ip in [
            [127, 0, 0, 1],
            [10, 0, 0, 1],
            [169, 254, 1, 1],
            [100, 64, 0, 1],
            [192, 0, 2, 1],
            [224, 0, 0, 1],
        ] {
            assert!(!public_ip(ip.into()));
        }
        assert!(!public_ip(std::net::Ipv6Addr::LOCALHOST.into()));
        assert!(public_ip([8, 8, 8, 8].into()));
        for url in [
            "https://cwwp2.dot.ca.gov.evil/data/d4/cctv/image/a/a.jpg",
            "https://cwwp2.dot.ca.gov/data/d4/cctv/image/a/../a.jpg",
            "https://cwwp2.dot.ca.gov/data/d4/cctv/image/a/a.jpg?next=evil",
        ] {
            assert!(!image_url(url, 4));
        }
        Ok(())
    }
    #[test]
    fn cancelled_completion_cannot_publish_or_free_a_new_request_slot() -> Result<()> {
        let mut state = TrafficState::default();
        let (token, _) = state.begin_catalog(4)?;
        state.cancel();
        assert!(state.begin_catalog(3).is_err());
        let catalog = parse_catalog(br#"{"data":[]}"#.to_vec(), 4, 1)?;
        state.finish_catalog(token, 4, Ok(catalog));
        assert!(state.catalog.is_none());
        assert!(!state.occupied);
        state.last_catalog_request = None;
        let (token, _) = state.begin_catalog(4)?;
        state.cancel();
        state.finish_catalog(
            token,
            4,
            Err(ToolError::new("traffic_http_404", "unavailable")),
        );
        assert!(state.blocked.contains(&catalog_url(4)));
        state.last_catalog_request = None;
        assert!(state.begin_catalog(4).is_err());
        Ok(())
    }
    #[test]
    fn url_and_origin_cutoffs_survive_reindexing_and_timeouts_stay_visible() -> Result<()> {
        let row = |index: &str, file: &str| serde_json::json!({"cctv":{"index":index,"inService":"true","location":{"district":"4","latitude":"37.8","longitude":"-122.2","locationName":"Highway view"},"imageData":{"static":{"currentImageURL":format!("{ORIGIN}/data/d4/cctv/image/cam/{file}.jpg")}}}});
        let catalog = |rows| {
            parse_catalog(
                serde_json::to_vec(&serde_json::json!({"data":rows})).map_err(error)?,
                4,
                1,
            )
        };
        let mut invalid = row("1", "shared");
        invalid["cctv"]["location"]["locationName"] = "".into();
        let initial = catalog(vec![invalid, row("1", "shared")])?;
        assert_eq!((initial.cameras.len(), initial.invalid), (1, 1));
        let mut state = TrafficState {
            catalog: Some(initial),
            visible: true,
            ..TrafficState::default()
        };
        state.select(Some("traffic:4:1"));
        let (token, _, camera, _, _) = state.begin_frame()?;
        state.finish_frame(
            token,
            &camera.image_url,
            Err(ToolError::new("traffic_http_404", "unavailable")),
        );
        state.catalog = Some(catalog(vec![row("2", "shared"), row("3", "different")])?);
        state.last_frame_request = None;
        state.select(Some("traffic:4:2"));
        assert!(
            state.begin_frame().is_err(),
            "renamed row restored the same unavailable resource"
        );
        state.select(Some("traffic:4:3"));
        let (token, _, camera, _, _) = state.begin_frame()?;
        state.finish_frame(
            token,
            &camera.image_url,
            Err(ToolError::new("deadline", "request timed out")),
        );
        assert_eq!(state.view().error_code, Some("deadline"));
        assert_eq!(state.view().error.as_deref(), Some("request timed out"));
        state.last_frame_request = None;
        let (token, _, camera, _, _) = state.begin_frame()?;
        state.cancel();
        state.finish_frame(
            token,
            &camera.image_url,
            Err(ToolError::new("traffic_http_403", "restricted")),
        );
        assert!(
            state.origin_blocked,
            "cancel must not discard the observed origin restriction"
        );
        assert!(
            state.view().error.is_none(),
            "cancelled failure became visible"
        );
        state.last_catalog_request = None;
        state.last_frame_request = None;
        assert!(state.begin_catalog(3).is_err());
        state.select(Some("traffic:4:2"));
        assert!(state.begin_frame().is_err());
        Ok(())
    }
}
