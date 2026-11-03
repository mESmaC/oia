//! Registered context layers for the Globe instrument (`#23` shell integration,
//! `CAM-06` through `#28`). Rust owns the registry, each layer's visibility, and the
//! local filter; the page draws what [`LayerView`] and [`CameraLayerView`] say and sends
//! intents. The only layer with features in this build is the mapped ALPR camera
//! directory over the bundled synthetic New Meridian fixture, evaluated by the merged
//! `oia-alpr-map` kernel: nothing here is a live extract, and every record carries the
//! fixture's synthetic marker. Mapped nodes are community assertions about equipment,
//! never observations; absence from the layer is never absence of cameras.

use crate::local_tools::{Result, ToolError};
use oia_alpr_map::{
    AlprCameraRecord, AlprCoverageStatus, AlprLocalFilter, AlprManufacturerClass, AlprQueryPolicy,
    GeoBBox, RawOsmNode, direction_wedge_polygon, evaluate_viewport_query, validate_alpr_records,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const LAYER_CASE: &str = "case";
pub const LAYER_ALPR: &str = "alpr-cameras";
/// Fixture schema accepted by this build.
pub const ALPR_FIXTURE_SCHEMA: &str = "oia.alpr.overpass-fixture.v1";
/// Elements accepted from the fixture before it is refused as oversized.
pub const MAX_FIXTURE_ELEMENTS: usize = 2_000;
const ALPR_FIXTURE_JSON: &str =
    include_str!("../../../../fixtures/cameras/alpr/new-meridian-corridor-fixture.json");
const ALPR_FIXTURE_PATH: &str = "fixtures/cameras/alpr/new-meridian-corridor-fixture.json";
const ALPR_SOURCE_NAME: &str = "Synthetic fixture";
const ALPR_ATTRIBUTION: &str = "Synthetic fixture authored for OIA; no OpenStreetMap data. A real extract would be © OpenStreetMap contributors (ODbL 1.0) with DeFlock credit where used.";
const ALPR_LICENSE: &str = "Repository fixture (MIT OR Apache-2.0); not an ODbL extract";
const ALPR_STATEMENT: &str = "Community-mapped equipment assertions, not observed hardware, feeds or plate reads. A wedge is an illustrative bearing from a numeric tag, not a calibrated field of view; no wedge means the bearing is not recorded. Absence from this layer is not absence of cameras.";

#[derive(Clone, Debug, Deserialize)]
struct FixtureViewport {
    south: f64,
    west: f64,
    north: f64,
    east: f64,
}

#[derive(Clone, Debug, Deserialize)]
struct FixtureElement {
    #[serde(rename = "type")]
    element_type: String,
    id: i64,
    #[serde(default)]
    synthetic: bool,
    lat: f64,
    lon: f64,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
struct AlprFixture {
    schema: String,
    origin: String,
    #[serde(default)]
    synthetic: bool,
    description: String,
    viewport: FixtureViewport,
    elements: Vec<FixtureElement>,
}

/// The parsed fixture: raw nodes for the kernel plus the extent it was written for.
#[derive(Clone, Debug)]
pub struct AlprFixtureData {
    pub description: String,
    pub viewport: GeoBBox,
    pub nodes: Vec<RawOsmNode>,
}

fn tag(tags: &BTreeMap<String, String>, key: &str) -> Option<String> {
    tags.get(key).cloned()
}

/// Parses the bundled Overpass-shaped fixture into kernel input. Only a synthetic fixture
/// is accepted, so the layer can never present fixture nodes as mapped evidence.
pub fn parse_alpr_fixture(json: &str) -> Result<AlprFixtureData> {
    let fixture: AlprFixture = serde_json::from_str(json).map_err(|e| {
        ToolError::new(
            "fixture_invalid",
            format!("ALPR fixture does not parse: {e}"),
        )
    })?;
    if fixture.schema != ALPR_FIXTURE_SCHEMA {
        return Err(ToolError::new(
            "fixture_invalid",
            format!(
                "ALPR fixture schema {:?} is not {ALPR_FIXTURE_SCHEMA:?}.",
                fixture.schema
            ),
        ));
    }
    if !fixture.synthetic || fixture.origin != "synthetic-fixture" {
        return Err(ToolError::new(
            "fixture_invalid",
            "This build draws only synthetic ALPR fixtures.",
        ));
    }
    if fixture.elements.len() > MAX_FIXTURE_ELEMENTS {
        return Err(ToolError::new(
            "fixture_invalid",
            format!(
                "ALPR fixture has {} elements; the limit is {MAX_FIXTURE_ELEMENTS}.",
                fixture.elements.len()
            ),
        ));
    }
    let v = fixture.viewport;
    let viewport = GeoBBox::new(v.south, v.west, v.north, v.east)
        .map_err(|e| ToolError::new("fixture_invalid", format!("ALPR fixture viewport: {e}")))?;
    let nodes = fixture
        .elements
        .iter()
        .map(|e| RawOsmNode {
            element_type: e.element_type.clone(),
            osm_id: e.id,
            latitude_deg: e.lat,
            longitude_deg: e.lon,
            coordinate_zoom: None,
            surveillance_type: tag(&e.tags, "surveillance:type"),
            manufacturer: tag(&e.tags, "manufacturer"),
            brand: tag(&e.tags, "brand"),
            operator: tag(&e.tags, "operator"),
            camera_type: tag(&e.tags, "camera:type"),
            surveillance_zone: tag(&e.tags, "surveillance:zone"),
            camera_direction: tag(&e.tags, "camera:direction"),
            direction: tag(&e.tags, "direction"),
            reference: tag(&e.tags, "ref"),
            check_date: tag(&e.tags, "check_date"),
            survey_date: tag(&e.tags, "survey:date"),
            osm_timestamp: e.timestamp.clone(),
            source: tag(&e.tags, "source"),
            note: tag(&e.tags, "note"),
            oia_synthetic: tag(&e.tags, "oia:synthetic"),
            synthetic: e.synthetic,
        })
        .collect();
    Ok(AlprFixtureData {
        description: fixture.description,
        viewport,
        nodes,
    })
}

/// Local filter ids as the page sends them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlprFilterId {
    All,
    FlockSafety,
    OtherNamed,
    Unspecified,
    WithBearing,
}

impl AlprFilterId {
    pub const ALL: [Self; 5] = [
        Self::All,
        Self::FlockSafety,
        Self::OtherNamed,
        Self::Unspecified,
        Self::WithBearing,
    ];
    fn kernel(self) -> AlprLocalFilter {
        match self {
            Self::All => AlprLocalFilter::AllMappedAlpr,
            Self::FlockSafety => AlprLocalFilter::FlockSafetyOnly,
            Self::OtherNamed => AlprLocalFilter::OtherNamedOnly,
            Self::Unspecified => AlprLocalFilter::UnspecifiedOnly,
            Self::WithBearing => AlprLocalFilter::WithBearingOnly,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::All => "All mapped ALPR",
            Self::FlockSafety => "Flock Safety only",
            Self::OtherNamed => "Other named vendor",
            Self::Unspecified => "Vendor unspecified",
            Self::WithBearing => "Bearing recorded only",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct LayerExtent {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
}

/// One registry entry as the page sees it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LayerView {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: &'static str,
    pub visible: bool,
    pub origin_class: &'static str,
    pub requirement: &'static str,
    pub source_label: String,
    pub fixture_path: Option<&'static str>,
    pub statement: &'static str,
    pub attribution: &'static str,
    pub license: &'static str,
    pub extent: Option<LayerExtent>,
    /// Features drawn after the local filter; `total` is before it.
    pub shown: usize,
    pub total: usize,
    pub coverage: String,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct WedgeView {
    pub geometry_class: &'static str,
    pub bearing_deg: f64,
    pub radius_m: f64,
    pub half_angle_deg: f64,
    pub left_lat_deg: f64,
    pub left_lon_deg: f64,
    pub right_lat_deg: f64,
    pub right_lon_deg: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CameraFeature {
    pub id: String,
    pub display_id: String,
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub manufacturer: Option<String>,
    pub manufacturer_class: &'static str,
    pub manufacturer_label: String,
    pub operator: Option<String>,
    pub camera_type: Option<String>,
    pub zone: Option<String>,
    pub direction_deg: Option<f64>,
    pub wedge: Option<WedgeView>,
    pub reference: Option<String>,
    /// Mapper's field check date, kept apart from the OSM edit timestamp.
    pub last_verified: Option<String>,
    pub osm_timestamp: Option<String>,
    pub source_tag: Option<String>,
    pub note: Option<String>,
    pub synthetic: bool,
    pub details: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct BreakdownView {
    pub total_in_viewport: usize,
    pub flock_safety: usize,
    pub other_named: usize,
    pub unspecified: usize,
    pub with_bearing: usize,
    pub verified_date_present: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FilterOption {
    pub id: AlprFilterId,
    pub label: &'static str,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CameraLayerView {
    pub layer_id: &'static str,
    pub source_name: &'static str,
    pub description: String,
    pub coverage_status: String,
    pub detail_zoom: Option<u8>,
    pub breakdown: BreakdownView,
    pub filters: Vec<FilterOption>,
    pub active_filter: AlprFilterId,
    pub omitted_by_filter: usize,
    pub query_limit: usize,
    pub wedge_radius_m: f64,
    pub wedge_half_angle_deg: f64,
    pub cameras: Vec<CameraFeature>,
}

fn camera_feature(record: &AlprCameraRecord, policy: AlprQueryPolicy) -> CameraFeature {
    let wedge = direction_wedge_polygon(record, policy).map(|w| WedgeView {
        geometry_class: "illustrative_bearing_wedge",
        bearing_deg: w.bearing_deg,
        radius_m: w.radius_m,
        half_angle_deg: w.half_angle_deg,
        left_lat_deg: w.left_lat_deg,
        left_lon_deg: w.left_lon_deg,
        right_lat_deg: w.right_lat_deg,
        right_lon_deg: w.right_lon_deg,
    });
    CameraFeature {
        id: record.id.clone(),
        display_id: record.display_id(),
        lat_deg: record.latitude_deg,
        lon_deg: record.longitude_deg,
        manufacturer: record.manufacturer.clone(),
        manufacturer_class: match record.manufacturer_class {
            AlprManufacturerClass::FlockSafety => "flock_safety",
            AlprManufacturerClass::OtherNamed(_) => "other_named",
            AlprManufacturerClass::Unspecified => "unspecified",
        },
        manufacturer_label: record.manufacturer_class.short_label().to_owned(),
        operator: record.operator.clone(),
        camera_type: record.camera_type.clone(),
        zone: record.zone.clone(),
        direction_deg: record.direction_deg,
        wedge,
        reference: record.reference.clone(),
        last_verified: record.last_verified.clone(),
        osm_timestamp: record.osm_timestamp.clone(),
        source_tag: record.source_tag.clone(),
        note: record.note.clone(),
        synthetic: record.synthetic,
        details: record.label_details(ALPR_SOURCE_NAME),
    }
}

fn coverage_text(status: AlprCoverageStatus) -> String {
    match status {
        AlprCoverageStatus::ZoomRequired => {
            "zoom required: extent exceeds the city-scale limit".to_owned()
        }
        AlprCoverageStatus::NoCoverage => {
            "no coverage: extent is outside the extract bounds".to_owned()
        }
        AlprCoverageStatus::Empty => {
            "empty: no mapped ALPR node in the extent (not an absence of cameras)".to_owned()
        }
        AlprCoverageStatus::Complete => "complete within the extent".to_owned(),
        AlprCoverageStatus::Saturated {
            total_matching,
            retained,
            omitted,
        } => format!("saturated: {retained} of {total_matching} retained, {omitted} omitted"),
    }
}

/// Rust-owned layer state: visibility per layer and the ALPR local filter.
#[derive(Clone, Debug)]
pub struct LayerState {
    alpr: std::result::Result<AlprFixtureData, ToolError>,
    alpr_visible: bool,
    alpr_filter: AlprFilterId,
}

impl Default for LayerState {
    fn default() -> Self {
        Self {
            alpr: parse_alpr_fixture(ALPR_FIXTURE_JSON),
            alpr_visible: false,
            alpr_filter: AlprFilterId::All,
        }
    }
}

impl LayerState {
    pub fn set_visible(&mut self, layer_id: &str, visible: bool) -> Result<()> {
        match layer_id {
            LAYER_ALPR => {
                self.alpr_visible = visible;
                Ok(())
            }
            LAYER_CASE => Err(ToolError::new(
                "layer_fixed",
                "The case layer is always drawn; use Clean evidence to hide overlays.",
            )),
            other => Err(ToolError::new(
                "layer_unknown",
                format!("{other:?} is not a registered layer."),
            )),
        }
    }

    pub fn set_alpr_filter(&mut self, filter: AlprFilterId) {
        self.alpr_filter = filter;
    }

    /// Whether `id` is shaped like a camera feature id of the ALPR layer.
    pub fn is_camera_id(id: &str) -> bool {
        id.starts_with("alpr:")
    }

    /// Whether `id` names a camera feature that is drawn right now: the layer is visible
    /// and the kernel's current evaluation (local filter applied) returns the feature.
    /// A hidden, filtered-out or excluded node is never selectable.
    pub fn camera_selectable(&self, id: &str) -> bool {
        self.alpr_visible
            && self
                .views(None, 0)
                .1
                .is_some_and(|c| c.cameras.iter().any(|f| f.id == id))
    }

    /// Evaluates the ALPR layer with the kernel. Errors are reported in the layer entry,
    /// never swallowed into an empty layer.
    pub fn views(
        &self,
        case_label: Option<&str>,
        case_count: usize,
    ) -> (Vec<LayerView>, Option<CameraLayerView>) {
        let mut layers = vec![LayerView {
            id: LAYER_CASE,
            label: "Case records",
            kind: "case",
            visible: true,
            origin_class: "synthetic",
            requirement: "GEO-01",
            source_label: case_label.unwrap_or("no case").to_owned(),
            fixture_path: Some("fixtures/globe/synthetic-valley-case.json"),
            statement: "Recorded points, approximate places and regions from the open case; a record without geometry stays in the not-plotted tray.",
            attribution: "Bundled synthetic case",
            license: "Repository fixture (MIT OR Apache-2.0)",
            extent: None,
            shown: case_count,
            total: case_count,
            coverage: "all records of the open case".to_owned(),
            error: None,
        }];
        let mut cameras = None;
        let policy = AlprQueryPolicy::default_city_scale();
        let mut entry = LayerView {
            id: LAYER_ALPR,
            label: "ALPR camera directory",
            kind: "cameras",
            visible: self.alpr_visible,
            origin_class: "synthetic",
            requirement: "CAM-06",
            source_label: "New Meridian corridor (synthetic fixture)".to_owned(),
            fixture_path: Some(ALPR_FIXTURE_PATH),
            statement: ALPR_STATEMENT,
            attribution: ALPR_ATTRIBUTION,
            license: ALPR_LICENSE,
            extent: None,
            shown: 0,
            total: 0,
            coverage: "not evaluated".to_owned(),
            error: None,
        };
        match &self.alpr {
            Err(error) => entry.error = Some(format!("{ALPR_FIXTURE_PATH}: {}", error.message)),
            Ok(data) => {
                entry.extent = Some(LayerExtent {
                    south: data.viewport.south(),
                    west: data.viewport.west(),
                    north: data.viewport.north(),
                    east: data.viewport.east(),
                });
                let evaluated = evaluate_viewport_query(
                    data.viewport,
                    Some(data.viewport),
                    &data.nodes,
                    self.alpr_filter.kernel(),
                    policy,
                    || None,
                )
                .and_then(|snapshot| validate_alpr_records(&snapshot.records).map(|()| snapshot));
                match evaluated {
                    Err(error) => entry.error = Some(format!("ALPR kernel: {error}")),
                    Ok(snapshot) => {
                        entry.shown = snapshot.records.len();
                        entry.total = snapshot.breakdown.total_in_viewport;
                        entry.coverage = coverage_text(snapshot.coverage_status);
                        let b = snapshot.breakdown;
                        cameras = Some(CameraLayerView {
                            layer_id: LAYER_ALPR,
                            source_name: ALPR_SOURCE_NAME,
                            description: data.description.clone(),
                            coverage_status: coverage_text(snapshot.coverage_status),
                            detail_zoom: snapshot.detail_zoom,
                            breakdown: BreakdownView {
                                total_in_viewport: b.total_in_viewport,
                                flock_safety: b.flock_safety,
                                other_named: b.other_named,
                                unspecified: b.unspecified,
                                with_bearing: b.with_bearing,
                                verified_date_present: b.verified_date_present,
                            },
                            filters: AlprFilterId::ALL
                                .iter()
                                .map(|id| FilterOption {
                                    id: *id,
                                    label: id.label(),
                                    active: *id == self.alpr_filter,
                                })
                                .collect(),
                            active_filter: self.alpr_filter,
                            omitted_by_filter: snapshot.local_filter_omitted,
                            query_limit: policy.query_limit(),
                            wedge_radius_m: policy.direction_cone_m(),
                            wedge_half_angle_deg: policy.direction_cone_half_angle_deg(),
                            cameras: snapshot
                                .records
                                .iter()
                                .map(|r| camera_feature(r, policy))
                                .collect(),
                        });
                    }
                }
            }
        }
        layers.push(entry);
        (layers, cameras)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // CAM-06 / AGENTS.md §2: the bundled fixture reaches the page only through the kernel,
    // every feature keeps its synthetic marker, a wedge exists only where a numeric bearing
    // is tagged, and the non-ALPR node is excluded rather than drawn.
    #[test]
    fn alpr_layer_keeps_markers_and_bearing_absence() {
        let state = LayerState::default();
        let (layers, cameras) = state.views(Some("case"), 4);
        let entry = layers
            .iter()
            .find(|l| l.id == LAYER_ALPR)
            .map(|l| l.error.clone());
        assert_eq!(entry, Some(None));
        let cameras = cameras
            .as_ref()
            .map(|c| &c.cameras)
            .cloned()
            .unwrap_or_default();
        assert!(!cameras.is_empty());
        assert!(
            cameras
                .iter()
                .all(|c| c.synthetic && c.details.iter().any(|d| d.contains("SYNTHETIC")))
        );
        assert!(
            cameras
                .iter()
                .all(|c| c.wedge.is_some() == c.direction_deg.is_some())
        );
        assert!(cameras.iter().any(|c| c.wedge.is_none()));
        assert!(cameras.iter().all(|c| c.id != "alpr:900000000199"));
        assert!(!state.camera_selectable("alpr:900000000101")); // layer hidden by default
        let mut shown = state.clone();
        shown.alpr_visible = true;
        assert!(shown.camera_selectable("alpr:900000000101"));
        assert!(!shown.camera_selectable("alpr:900000000199")); // non-ALPR node, excluded by the kernel
        assert!(!shown.camera_selectable("alpr:1"));
    }

    // A non-synthetic or oversized fixture is refused, not drawn.
    #[test]
    fn non_synthetic_fixture_is_refused() {
        let observed =
            ALPR_FIXTURE_JSON.replacen("\"synthetic\": true,", "\"synthetic\": false,", 1);
        assert_eq!(
            parse_alpr_fixture(&observed).err().map(|e| e.code),
            Some("fixture_invalid")
        );
        let mut state = LayerState::default();
        assert!(state.set_visible("nope", true).is_err());
        assert!(state.set_visible(LAYER_ALPR, true).is_ok());
    }
}
