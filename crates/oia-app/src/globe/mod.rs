//! Globe instrument state (`#23`): the bundled synthetic case, the registered imagery
//! sources, the camera view and the selection. Rust owns this state; the page draws the
//! [`GlobeView`] it is sent and sends intents (ADR 0008 §4 rule 1).
//!
//! Coordinate and time types here are provisional pending `#12` (`oia-time-geo`). They are
//! kept minimal and validated (finite, in range, bounded text) so that no unchecked value
//! reaches the renderer, and a record without recorded geometry is never given a position.

pub mod imagery;
pub mod layers;

use crate::local_tools::{Result, ToolError};
use imagery::{CivilDate, ImagerySelection, ImageryView};
use layers::{AlprFilterId, CameraLayerView, LayerState, LayerView};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Fixture schema accepted by this build.
pub const FIXTURE_SCHEMA: &str = "oia.fixture.globe-case/1";
pub const MAX_RECORDS: usize = 512;
pub const MAX_RING_POINTS: usize = 256;
pub const MIN_RING_POINTS: usize = 3;
pub const MAX_TEXT_BYTES: usize = 2_048;
/// Longest record id accepted over IPC.
pub const MAX_ID_BYTES: usize = 256;
pub const MIN_CAMERA_HEIGHT_M: f64 = 50.0;
pub const MAX_CAMERA_HEIGHT_M: f64 = 60_000_000.0;
/// Starting height over the case anchor.
pub const DEFAULT_CAMERA_HEIGHT_M: f64 = 80_000.0;
/// Fallback view when no case can be drawn.
pub const WORLD_CAMERA_HEIGHT_M: f64 = 20_000_000.0;
/// The CesiumJS build pinned by `scripts/fetch-ui-vendor.*`.
pub const RENDERER_VERSION: &str = "1.147.0";

const FIXTURE_JSON: &str = include_str!("../../../../fixtures/globe/synthetic-valley-case.json");
const FIXTURE_PATH: &str = "fixtures/globe/synthetic-valley-case.json";

fn text(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(ToolError::new(
            "fixture_invalid",
            format!("{name} must not be empty."),
        ));
    }
    if value.len() > MAX_TEXT_BYTES {
        return Err(ToolError::new(
            "fixture_invalid",
            format!(
                "{name} is {} bytes; the limit is {MAX_TEXT_BYTES}.",
                value.len()
            ),
        ));
    }
    Ok(())
}

fn finite(name: &str, value: f64, min: f64, max: f64) -> Result<f64> {
    if !value.is_finite() || value < min || value > max {
        return Err(ToolError::new(
            "value_out_of_range",
            format!("{name} must be a finite number between {min} and {max}; got {value}."),
        ));
    }
    Ok(value)
}

/// A WGS84 geodetic position in degrees. Provisional pending `#12`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wgs84Point {
    pub lat_deg: f64,
    pub lon_deg: f64,
}

impl Wgs84Point {
    pub fn validate(&self, name: &str) -> Result<()> {
        finite(&format!("{name}.lat_deg"), self.lat_deg, -90.0, 90.0)?;
        finite(&format!("{name}.lon_deg"), self.lon_deg, -180.0, 180.0)?;
        Ok(())
    }
}

/// Recorded geometry of a case record. Each variant states the basis for its drawing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordGeometry {
    /// A described position; the uncertainty radius is the author's bound, or unknown.
    Point {
        lat_deg: f64,
        lon_deg: f64,
        #[serde(default)]
        horizontal_uncertainty_m: Option<f64>,
        basis: String,
    },
    /// A named-place centroid with a radius; never a position where something happened.
    ApproximateCentroid {
        lat_deg: f64,
        lon_deg: f64,
        radius_m: f64,
        place_label: String,
        basis: String,
    },
    /// An area claim. It has no centre and is never reduced to one.
    Region {
        label: String,
        ring: Vec<Wgs84Point>,
        basis: String,
    },
}

impl RecordGeometry {
    fn validate(&self, name: &str) -> Result<()> {
        match self {
            Self::Point {
                lat_deg,
                lon_deg,
                horizontal_uncertainty_m,
                basis,
            } => {
                Wgs84Point {
                    lat_deg: *lat_deg,
                    lon_deg: *lon_deg,
                }
                .validate(name)?;
                if let Some(u) = horizontal_uncertainty_m {
                    finite(
                        &format!("{name}.horizontal_uncertainty_m"),
                        *u,
                        f64::MIN_POSITIVE,
                        20_000_000.0,
                    )?;
                }
                text(&format!("{name}.basis"), basis)
            }
            Self::ApproximateCentroid {
                lat_deg,
                lon_deg,
                radius_m,
                place_label,
                basis,
            } => {
                Wgs84Point {
                    lat_deg: *lat_deg,
                    lon_deg: *lon_deg,
                }
                .validate(name)?;
                finite(
                    &format!("{name}.radius_m"),
                    *radius_m,
                    f64::MIN_POSITIVE,
                    20_000_000.0,
                )?;
                text(&format!("{name}.place_label"), place_label)?;
                text(&format!("{name}.basis"), basis)
            }
            Self::Region { label, ring, basis } => {
                if ring.len() < MIN_RING_POINTS || ring.len() > MAX_RING_POINTS {
                    return Err(ToolError::new(
                        "fixture_invalid",
                        format!(
                            "{name}.ring has {} points; between {MIN_RING_POINTS} and {MAX_RING_POINTS} are accepted.",
                            ring.len()
                        ),
                    ));
                }
                for (i, point) in ring.iter().enumerate() {
                    point.validate(&format!("{name}.ring[{i}]"))?;
                }
                text(&format!("{name}.label"), label)?;
                text(&format!("{name}.basis"), basis)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimePrecision {
    /// A reported local window with both bounds.
    ApproximateWindow,
    /// A calendar date only; never promoted to an instant.
    CalendarDate,
    NotRecorded,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportedTime {
    pub text: String,
    pub precision: TimePrecision,
    #[serde(default)]
    pub window_start_local: Option<String>,
    #[serde(default)]
    pub window_end_local: Option<String>,
    #[serde(default)]
    pub zone: Option<String>,
}

impl ReportedTime {
    fn validate(&self, name: &str) -> Result<()> {
        text(&format!("{name}.text"), &self.text)?;
        let window = self.window_start_local.is_some() && self.window_end_local.is_some();
        match self.precision {
            TimePrecision::ApproximateWindow if !window || self.zone.is_none() => {
                Err(ToolError::new(
                    "fixture_invalid",
                    format!("{name}: an approximate window needs both bounds and a zone."),
                ))
            }
            TimePrecision::NotRecorded if window => Err(ToolError::new(
                "fixture_invalid",
                format!("{name}: a time that is not recorded cannot carry a window."),
            )),
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaseRecord {
    pub id: String,
    pub order: u32,
    pub label: String,
    pub kind: String,
    pub source_id: String,
    #[serde(default)]
    pub geometry: Option<RecordGeometry>,
    #[serde(default)]
    pub geometry_absent_reason: Option<String>,
    pub reported_time: ReportedTime,
    #[serde(default)]
    pub notes: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseSource {
    pub id: String,
    pub label: String,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseInterval {
    pub start_local: String,
    pub end_local: String,
    pub zone: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseClock {
    pub local: String,
    pub uncertainty_min: u32,
    pub zone: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaseHeader {
    pub id: String,
    pub label: String,
    pub anchor: Wgs84Point,
    pub interval: CaseInterval,
    pub clock: CaseClock,
}

/// The bundled fixture, parsed and validated.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GlobeCaseFixture {
    pub schema: String,
    pub origin_class: String,
    pub description: String,
    pub case: CaseHeader,
    pub sources: Vec<CaseSource>,
    pub records: Vec<CaseRecord>,
}

/// Parses and validates a globe case fixture. Only synthetic fixtures are accepted by
/// this build, so nothing it draws can be mistaken for an observation.
pub fn parse_fixture(json: &str) -> Result<GlobeCaseFixture> {
    let fixture: GlobeCaseFixture = serde_json::from_str(json)
        .map_err(|e| ToolError::new("fixture_invalid", format!("Fixture does not parse: {e}")))?;
    if fixture.schema != FIXTURE_SCHEMA {
        return Err(ToolError::new(
            "fixture_invalid",
            format!(
                "Fixture schema {:?} is not {FIXTURE_SCHEMA:?}.",
                fixture.schema
            ),
        ));
    }
    if fixture.origin_class != "synthetic" {
        return Err(ToolError::new(
            "fixture_invalid",
            format!(
                "This build draws only synthetic fixtures; origin_class is {:?}.",
                fixture.origin_class
            ),
        ));
    }
    text("description", &fixture.description)?;
    text("case.id", &fixture.case.id)?;
    text("case.label", &fixture.case.label)?;
    fixture.case.anchor.validate("case.anchor")?;
    text(
        "case.interval.start_local",
        &fixture.case.interval.start_local,
    )?;
    text("case.interval.end_local", &fixture.case.interval.end_local)?;
    text("case.interval.zone", &fixture.case.interval.zone)?;
    if fixture.case.interval.note.len() > MAX_TEXT_BYTES {
        return Err(ToolError::new(
            "fixture_invalid",
            format!("case.interval.note exceeds {MAX_TEXT_BYTES} bytes."),
        ));
    }
    text("case.clock.local", &fixture.case.clock.local)?;
    text("case.clock.zone", &fixture.case.clock.zone)?;
    if fixture.records.len() > MAX_RECORDS {
        return Err(ToolError::new(
            "fixture_invalid",
            format!(
                "{} records; the limit is {MAX_RECORDS}.",
                fixture.records.len()
            ),
        ));
    }
    let mut source_ids = BTreeSet::new();
    for source in &fixture.sources {
        text("sources[].id", &source.id)?;
        text("sources[].label", &source.label)?;
        text("sources[].kind", &source.kind)?;
        if !source_ids.insert(source.id.as_str()) {
            return Err(ToolError::new(
                "fixture_invalid",
                format!("Duplicate source id {:?}.", source.id),
            ));
        }
    }
    let mut ids = BTreeSet::new();
    let mut orders = BTreeSet::new();
    for record in &fixture.records {
        let name = format!("records[{}]", record.id);
        text(&format!("{name}.id"), &record.id)?;
        text(&format!("{name}.label"), &record.label)?;
        text(&format!("{name}.kind"), &record.kind)?;
        if record.notes.len() > MAX_TEXT_BYTES {
            return Err(ToolError::new(
                "fixture_invalid",
                format!("{name}.notes exceeds {MAX_TEXT_BYTES} bytes."),
            ));
        }
        if !ids.insert(record.id.as_str()) {
            return Err(ToolError::new(
                "fixture_invalid",
                format!("Duplicate record id {:?}.", record.id),
            ));
        }
        if !orders.insert(record.order) {
            return Err(ToolError::new(
                "fixture_invalid",
                format!("Duplicate display order {}.", record.order),
            ));
        }
        if !source_ids.contains(record.source_id.as_str()) {
            return Err(ToolError::new(
                "fixture_invalid",
                format!("{name} cites unknown source {:?}.", record.source_id),
            ));
        }
        record
            .reported_time
            .validate(&format!("{name}.reported_time"))?;
        match (&record.geometry, &record.geometry_absent_reason) {
            (Some(geometry), None) => geometry.validate(&format!("{name}.geometry"))?,
            (None, Some(reason)) => text(&format!("{name}.geometry_absent_reason"), reason)?,
            _ => {
                return Err(ToolError::new(
                    "fixture_invalid",
                    format!(
                        "{name} must have either geometry or a geometry_absent_reason, not both or neither."
                    ),
                ));
            }
        }
    }
    Ok(fixture)
}

/// The camera as the page reports it after a move, validated before it is kept.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraView {
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub height_m: f64,
    pub heading_deg: f64,
    pub pitch_deg: f64,
}

impl CameraView {
    pub fn validate(&self) -> Result<()> {
        finite("camera.lat_deg", self.lat_deg, -90.0, 90.0)?;
        finite("camera.lon_deg", self.lon_deg, -180.0, 180.0)?;
        finite(
            "camera.height_m",
            self.height_m,
            MIN_CAMERA_HEIGHT_M,
            MAX_CAMERA_HEIGHT_M,
        )?;
        finite("camera.heading_deg", self.heading_deg, 0.0, 360.0)?;
        finite("camera.pitch_deg", self.pitch_deg, -90.0, 90.0)?;
        Ok(())
    }

    fn over(point: Wgs84Point, height_m: f64) -> Self {
        Self {
            lat_deg: point.lat_deg,
            lon_deg: point.lon_deg,
            height_m,
            heading_deg: 0.0,
            pitch_deg: -90.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CaseView {
    pub id: String,
    pub label: String,
    pub origin_class: String,
    pub description: String,
    pub fixture_path: &'static str,
    pub anchor: Wgs84Point,
    pub interval: CaseInterval,
    pub clock: CaseClock,
    pub sources: Vec<CaseSource>,
    pub record_count: usize,
    pub plotted_count: usize,
    pub not_plotted_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlottedRecord {
    pub order: u32,
    pub id: String,
    pub label: String,
    pub kind: String,
    pub source_id: String,
    pub source_label: String,
    pub geometry: RecordGeometry,
    pub reported_time: ReportedTime,
    pub notes: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NotPlottedRecord {
    pub order: u32,
    pub id: String,
    pub label: String,
    pub kind: String,
    pub source_id: String,
    pub source_label: String,
    pub reason: String,
    pub reported_time: ReportedTime,
    pub notes: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RendererInfo {
    pub name: &'static str,
    pub pinned_version: &'static str,
    pub ion: &'static str,
    pub terrain: &'static str,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GlobeLimits {
    pub max_records: usize,
    pub max_ring_points: usize,
    pub min_camera_height_m: f64,
    pub max_camera_height_m: f64,
}

/// Everything the page needs to draw the instrument. It computes nothing from this.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GlobeView {
    pub fixture_error: Option<String>,
    pub case: Option<CaseView>,
    pub plotted: Vec<PlottedRecord>,
    pub not_plotted: Vec<NotPlottedRecord>,
    pub selection: Option<String>,
    pub camera: CameraView,
    pub home: CameraView,
    pub imagery: ImageryView,
    /// Registered context layers, in registry order; the case layer is first.
    pub layers: Vec<LayerView>,
    /// The ALPR camera layer's features and kernel evaluation, present whenever the
    /// fixture parsed; drawn by the page only while its layer is visible.
    pub cameras: Option<CameraLayerView>,
    pub renderer: RendererInfo,
    pub limits: GlobeLimits,
}

/// Rust-owned instrument state.
#[derive(Clone, Debug)]
pub struct GlobeState {
    fixture: std::result::Result<GlobeCaseFixture, ToolError>,
    /// A case record id or a camera feature id (`alpr:<osm id>`).
    selection: Option<String>,
    camera: CameraView,
    layers: LayerState,
    /// Resolved once, when selected, against the date given then. Viewing never re-validates,
    /// so a clock that moves cannot turn an accepted selection into a failing view. `None`
    /// only if the registry's default basemap cannot be resolved, which is a build defect.
    imagery: Option<ImageryView>,
}

impl Default for GlobeState {
    fn default() -> Self {
        Self::new(CivilDate::today_utc())
    }
}

impl GlobeState {
    /// Builds the state from the bundled fixture, resolving the default imagery for `today`.
    #[must_use]
    pub fn new(today: CivilDate) -> Self {
        Self::from_fixture(parse_fixture(FIXTURE_JSON), today)
    }

    fn from_fixture(
        fixture: std::result::Result<GlobeCaseFixture, ToolError>,
        today: CivilDate,
    ) -> Self {
        let camera = Self::home_for(&fixture);
        // The default selection has no date and a registered basemap, so this resolves unless
        // the registry itself is broken; `view` then reports that instead of guessing.
        let imagery = imagery::resolve(&ImagerySelection::default(), today).ok();
        Self {
            fixture,
            selection: None,
            camera,
            layers: LayerState::default(),
            imagery,
        }
    }

    fn home_for(fixture: &std::result::Result<GlobeCaseFixture, ToolError>) -> CameraView {
        match fixture {
            Ok(f) => CameraView::over(f.case.anchor, DEFAULT_CAMERA_HEIGHT_M),
            Err(_) => CameraView::over(
                Wgs84Point {
                    lat_deg: 0.0,
                    lon_deg: 0.0,
                },
                WORLD_CAMERA_HEIGHT_M,
            ),
        }
    }

    /// Selects a record by id, or clears the selection. Unknown ids are refused.
    pub fn select(&mut self, record_id: Option<String>) -> Result<()> {
        if let Some(id) = &record_id {
            if id.len() > MAX_ID_BYTES {
                return Err(ToolError::new(
                    "unknown_record",
                    format!(
                        "A record id of {} bytes exceeds the {MAX_ID_BYTES}-byte limit.",
                        id.len()
                    ),
                ));
            }
            let known = self
                .fixture
                .as_ref()
                .map(|f| f.records.iter().any(|r| &r.id == id))
                .unwrap_or(false)
                || self.layers.camera_selectable(id);
            if !known {
                return Err(ToolError::new(
                    "unknown_record",
                    format!(
                        "{id:?} is not a record of the open case or a drawn feature of a visible layer."
                    ),
                ));
            }
        }
        self.selection = record_id;
        Ok(())
    }

    /// Shows or hides a registered layer. Hiding a layer clears a selection inside it,
    /// so a target card never describes something that is not drawn.
    pub fn set_layer_visible(&mut self, layer_id: &str, visible: bool) -> Result<()> {
        self.layers.set_visible(layer_id, visible)?;
        if !visible
            && self
                .selection
                .as_deref()
                .is_some_and(LayerState::is_camera_id)
        {
            self.selection = None;
        }
        Ok(())
    }

    /// Applies the ALPR local filter. A selected camera that the filter omits is deselected.
    pub fn set_alpr_filter(&mut self, filter: AlprFilterId) -> Result<()> {
        self.layers.set_alpr_filter(filter);
        if let Some(id) = self.selection.as_deref()
            && LayerState::is_camera_id(id)
            && !self.layers.camera_selectable(id)
        {
            self.selection = None;
        }
        Ok(())
    }

    pub fn set_camera(&mut self, camera: CameraView) -> Result<()> {
        camera.validate()?;
        self.camera = camera;
        Ok(())
    }

    /// Validates the selection against the registry and `today`, then keeps the resolved view.
    pub fn set_imagery(&mut self, selection: ImagerySelection, today: CivilDate) -> Result<()> {
        self.imagery = Some(imagery::resolve(&selection, today)?);
        Ok(())
    }

    pub fn view(&self) -> Result<GlobeView> {
        let imagery = self.imagery.clone().ok_or_else(|| {
            ToolError::new(
                "imagery_unavailable",
                "The default imagery source could not be resolved; the registry in this build is misconfigured.",
            )
        })?;
        let mut view = GlobeView {
            fixture_error: None,
            case: None,
            plotted: Vec::new(),
            not_plotted: Vec::new(),
            selection: self.selection.clone(),
            camera: self.camera,
            home: Self::home_for(&self.fixture),
            imagery,
            layers: Vec::new(),
            cameras: None,
            renderer: RendererInfo {
                name: "CesiumJS",
                pinned_version: RENDERER_VERSION,
                ion: "disabled: no token, no Ion assets",
                terrain: "WGS84 ellipsoid; no terrain source is registered",
            },
            limits: GlobeLimits {
                max_records: MAX_RECORDS,
                max_ring_points: MAX_RING_POINTS,
                min_camera_height_m: MIN_CAMERA_HEIGHT_M,
                max_camera_height_m: MAX_CAMERA_HEIGHT_M,
            },
        };
        let fixture = match &self.fixture {
            Ok(fixture) => fixture,
            Err(error) => {
                view.fixture_error = Some(format!("{FIXTURE_PATH}: {}", error.message));
                let (layers, cameras) = self.layers.views(None, 0);
                view.layers = layers;
                view.cameras = cameras;
                return Ok(view);
            }
        };
        let source_label = |id: &str| {
            fixture
                .sources
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.label.clone())
                .unwrap_or_default()
        };
        let mut records: Vec<&CaseRecord> = fixture.records.iter().collect();
        records.sort_by_key(|r| r.order);
        for record in records {
            match (&record.geometry, &record.geometry_absent_reason) {
                (Some(geometry), _) => view.plotted.push(PlottedRecord {
                    order: record.order,
                    id: record.id.clone(),
                    label: record.label.clone(),
                    kind: record.kind.clone(),
                    source_id: record.source_id.clone(),
                    source_label: source_label(&record.source_id),
                    geometry: geometry.clone(),
                    reported_time: record.reported_time.clone(),
                    notes: record.notes.clone(),
                }),
                (None, reason) => view.not_plotted.push(NotPlottedRecord {
                    order: record.order,
                    id: record.id.clone(),
                    label: record.label.clone(),
                    kind: record.kind.clone(),
                    source_id: record.source_id.clone(),
                    source_label: source_label(&record.source_id),
                    reason: reason
                        .clone()
                        .unwrap_or_else(|| "No geometry recorded.".to_owned()),
                    reported_time: record.reported_time.clone(),
                    notes: record.notes.clone(),
                }),
            }
        }
        view.case = Some(CaseView {
            id: fixture.case.id.clone(),
            label: fixture.case.label.clone(),
            origin_class: fixture.origin_class.clone(),
            description: fixture.description.clone(),
            fixture_path: FIXTURE_PATH,
            anchor: fixture.case.anchor,
            interval: fixture.case.interval.clone(),
            clock: fixture.case.clock.clone(),
            sources: fixture.sources.clone(),
            record_count: fixture.records.len(),
            plotted_count: view.plotted.len(),
            not_plotted_count: view.not_plotted.len(),
        });
        let (layers, cameras) = self
            .layers
            .views(Some(&fixture.case.label), fixture.records.len());
        view.layers = layers;
        view.cameras = cameras;
        Ok(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // GEO-01 / AGENTS.md §2: the bundled fixture parses, a record without recorded
    // geometry is listed as not plotted with its reason and never receives a position,
    // and a region keeps its ring rather than acquiring a centre.
    #[test]
    fn bundled_fixture_separates_plotted_from_not_plotted() -> Result<()> {
        let state = GlobeState::default();
        let view = state.view()?;
        assert_eq!(view.fixture_error, None);
        let case = view
            .case
            .as_ref()
            .ok_or_else(|| ToolError::new("test", "no case"))?;
        assert_eq!(case.origin_class, "synthetic");
        assert_eq!(
            case.record_count,
            view.plotted.len() + view.not_plotted.len()
        );
        assert!(
            view.not_plotted
                .iter()
                .any(|r| r.id == "acct-b" && !r.reason.is_empty())
        );
        assert!(view.plotted.iter().all(|r| r.id != "acct-b"));
        assert!(view.plotted.iter().any(|r| {
            r.id == "acct-c"
                && matches!(&r.geometry, RecordGeometry::Region { ring, .. } if ring.len() >= 3)
        }));
        assert_eq!(view.camera, view.home);
        Ok(())
    }

    // AGENTS.md §5: malformed fixtures and camera values are refused, not clamped or defaulted.
    #[test]
    fn invalid_fixture_and_camera_values_are_refused() {
        let base = FIXTURE_JSON;
        let no_reason = base.replacen(
            "\"geometry_absent_reason\": \"Position not recorded in the source.\",",
            "",
            1,
        );
        assert_eq!(
            parse_fixture(&no_reason).err().map(|e| e.code),
            Some("fixture_invalid")
        );
        let nan_lat = base.replacen("\"lat_deg\": 39.4083", "\"lat_deg\": 139.4083", 1);
        assert_eq!(
            parse_fixture(&nan_lat).err().map(|e| e.code),
            Some("value_out_of_range")
        );
        let observed = base.replacen(
            "\"origin_class\": \"synthetic\"",
            "\"origin_class\": \"observed\"",
            1,
        );
        assert_eq!(
            parse_fixture(&observed).err().map(|e| e.code),
            Some("fixture_invalid")
        );

        let mut state = GlobeState::default();
        let before = state.camera;
        let bad = CameraView {
            height_m: f64::NAN,
            ..before
        };
        assert_eq!(
            state.set_camera(bad).err().map(|e| e.code),
            Some("value_out_of_range")
        );
        assert_eq!(state.camera, before);
        assert_eq!(
            state.select(Some("nope".to_owned())).err().map(|e| e.code),
            Some("unknown_record")
        );
        assert!(state.select(Some("acct-b".to_owned())).is_ok());
    }

    // #23 layers: a camera feature is selectable only through the registry, and hiding
    // its layer or filtering it out clears that selection rather than leaving a card
    // for geometry that is no longer drawn.
    #[test]
    fn layer_visibility_and_filter_govern_camera_selection() -> Result<()> {
        let mut state = GlobeState::default();
        // Hidden layer: its features are not drawn, so they cannot be selected.
        assert_eq!(
            state
                .select(Some("alpr:900000000101".to_owned()))
                .err()
                .map(|e| e.code),
            Some("unknown_record")
        );
        state.set_layer_visible(layers::LAYER_ALPR, true)?;
        // The fixture's non-ALPR node is excluded by the kernel and stays unselectable.
        assert!(state.select(Some("alpr:900000000199".to_owned())).is_err());
        state.select(Some("alpr:900000000101".to_owned()))?;
        assert!(
            state
                .view()?
                .layers
                .iter()
                .any(|l| l.id == layers::LAYER_ALPR && l.visible)
        );
        state.set_alpr_filter(AlprFilterId::Unspecified)?;
        assert_eq!(state.view()?.selection, None);
        state.set_alpr_filter(AlprFilterId::All)?;
        state.select(Some("alpr:900000000101".to_owned()))?;
        state.set_layer_visible(layers::LAYER_ALPR, false)?;
        assert_eq!(state.view()?.selection, None);
        assert!(state.set_layer_visible(layers::LAYER_CASE, false).is_err());
        Ok(())
    }
}
