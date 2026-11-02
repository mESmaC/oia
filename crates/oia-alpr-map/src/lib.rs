//! Bounded ALPR and Flock camera directory normalization, viewport query,
//! coordinate precision retention, and illustrative bearing-wedge kernel (`CAM-06`).
//!
//! Selective Rust port and tightening of `src/layers/alpr/*` from God's Eye View
//! (`0d1a68160e532ba65dc34b48ea205de6de4028fc`, MIT License, Copyright (c)
//! Bilawal Sidhu). OpenStreetMap camera directory data remains licensed under
//! ODbL 1.0 (`© OpenStreetMap contributors`), distinct from OIA's `MIT OR Apache-2.0`
//! code license (`D14`).
//!
//! Under `CAM-06` (distinct from `CAM-01`/`CAM-02` viewable camera feeds), mapped
//! ALPR records represent community-mapped equipment assertions and tags only,
//! never verified hardware deployments, live video feeds, license-plate read logs,
//! or exhaustive road coverage.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

/// Default maximum viewport span in degrees before requiring zoom-in.
pub const DEFAULT_MAX_VIEWPORT_DEGREES: f64 = 3.0;
/// Default outward grid snap in degrees for viewport query reuse.
pub const DEFAULT_QUERY_SNAP_DEGREES: f64 = 0.05;
/// Finest detail vector-tile zoom carrying attributed camera points.
pub const DEFAULT_DETAIL_MAX_ZOOM: u8 = 12;
/// Coarsest detail vector-tile zoom carrying attributed camera points.
pub const DEFAULT_DETAIL_MIN_ZOOM: u8 = 9;
/// Maximum detail tiles read per viewport per country extract.
pub const DEFAULT_DETAIL_MAX_TILES: usize = 16;
/// Default per-viewport record cap; reaching it marks coverage `Saturated`.
pub const DEFAULT_QUERY_LIMIT: usize = 1_500;
/// Default decoded source record ceiling across accepted tiles.
pub const DEFAULT_SOURCE_RECORD_LIMIT: usize = 6_000;
/// Default bounded coordinate precision cache capacity.
pub const DEFAULT_PRECISION_CACHE_LIMIT: usize = 20_000;
/// Default illustrative bearing wedge length in meters.
pub const DEFAULT_DIRECTION_CONE_M: f64 = 90.0;
/// Default illustrative bearing wedge half-angle in degrees.
pub const DEFAULT_DIRECTION_CONE_HALF_ANGLE_DEG: f64 = 20.0;
/// WGS84 mean spherical radius in meters used for illustrative bearing wedges.
pub const EARTH_MEAN_RADIUS_M: f64 = 6_371_008.8;

const WEB_MERCATOR_MAX_LAT_DEG: f64 = 85.051_128_779_806_6;
const RETRY_MIN_MS: u64 = 30_000;
const RETRY_CEIL_MS: u64 = 240_000;

/// Cooperative cancellation, deadline, or supersession reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    Cancelled,
    DeadlineExceeded,
    Superseded,
}

/// Typed errors returned by the ALPR directory and viewport kernel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AlprMapError {
    InvalidViewport(&'static str),
    InvalidPolicy(&'static str),
    InvalidRecord(&'static str),
    DuplicateRecordId(String),
    InvalidAttributionHref(&'static str),
    AllocationFailed,
    Stopped(StopReason),
}

impl fmt::Display for AlprMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidViewport(reason) => write!(formatter, "invalid ALPR viewport: {reason}"),
            Self::InvalidPolicy(reason) => write!(formatter, "invalid ALPR query policy: {reason}"),
            Self::InvalidRecord(reason) => {
                write!(formatter, "invalid ALPR camera record: {reason}")
            }
            Self::DuplicateRecordId(id) => {
                write!(formatter, "duplicate ALPR camera record id: {id}")
            }
            Self::InvalidAttributionHref(reason) => {
                write!(formatter, "invalid camera attribution URL: {reason}")
            }
            Self::AllocationFailed => write!(formatter, "memory allocation failed"),
            Self::Stopped(reason) => write!(formatter, "ALPR query stopped: {reason:?}"),
        }
    }
}

impl Error for AlprMapError {}

/// Validated operational bounds for city-scale ALPR directory queries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlprQueryPolicy {
    max_viewport_degrees: f64,
    query_snap_degrees: f64,
    detail_min_zoom: u8,
    detail_max_zoom: u8,
    detail_max_tiles: usize,
    query_limit: usize,
    source_record_limit: usize,
    precision_cache_limit: usize,
    direction_cone_m: f64,
    direction_cone_half_angle_deg: f64,
}

impl AlprQueryPolicy {
    /// Standard city-scale policy matching the documented OIA and God's Eye View contract.
    pub fn default_city_scale() -> Self {
        Self {
            max_viewport_degrees: DEFAULT_MAX_VIEWPORT_DEGREES,
            query_snap_degrees: DEFAULT_QUERY_SNAP_DEGREES,
            detail_min_zoom: DEFAULT_DETAIL_MIN_ZOOM,
            detail_max_zoom: DEFAULT_DETAIL_MAX_ZOOM,
            detail_max_tiles: DEFAULT_DETAIL_MAX_TILES,
            query_limit: DEFAULT_QUERY_LIMIT,
            source_record_limit: DEFAULT_SOURCE_RECORD_LIMIT,
            precision_cache_limit: DEFAULT_PRECISION_CACHE_LIMIT,
            direction_cone_m: DEFAULT_DIRECTION_CONE_M,
            direction_cone_half_angle_deg: DEFAULT_DIRECTION_CONE_HALF_ANGLE_DEG,
        }
    }

    /// Construct a policy with custom record and tile limits, rejecting zero or inverted bounds.
    pub fn with_limits(
        max_viewport_degrees: f64,
        query_snap_degrees: f64,
        detail_min_zoom: u8,
        detail_max_zoom: u8,
        detail_max_tiles: usize,
        query_limit: usize,
    ) -> Result<Self, AlprMapError> {
        if !max_viewport_degrees.is_finite()
            || max_viewport_degrees <= 0.0
            || max_viewport_degrees > 180.0
        {
            return Err(AlprMapError::InvalidPolicy(
                "max_viewport_degrees must be in (0, 180]",
            ));
        }
        if !query_snap_degrees.is_finite() || query_snap_degrees <= 0.0 || query_snap_degrees > 10.0
        {
            return Err(AlprMapError::InvalidPolicy(
                "query_snap_degrees must be in (0, 10]",
            ));
        }
        if detail_min_zoom == 0 || detail_min_zoom > detail_max_zoom || detail_max_zoom > 22 {
            return Err(AlprMapError::InvalidPolicy(
                "detail zoom range must satisfy 1 <= min <= max <= 22",
            ));
        }
        if detail_max_tiles == 0 {
            return Err(AlprMapError::InvalidPolicy(
                "detail_max_tiles must be greater than zero",
            ));
        }
        if query_limit == 0 {
            return Err(AlprMapError::InvalidPolicy(
                "query_limit must be greater than zero",
            ));
        }
        Ok(Self {
            max_viewport_degrees,
            query_snap_degrees,
            detail_min_zoom,
            detail_max_zoom,
            detail_max_tiles,
            query_limit,
            ..Self::default_city_scale()
        })
    }

    pub fn max_viewport_degrees(self) -> f64 {
        self.max_viewport_degrees
    }

    pub fn query_snap_degrees(self) -> f64 {
        self.query_snap_degrees
    }

    pub fn detail_min_zoom(self) -> u8 {
        self.detail_min_zoom
    }

    pub fn detail_max_zoom(self) -> u8 {
        self.detail_max_zoom
    }

    pub fn detail_max_tiles(self) -> usize {
        self.detail_max_tiles
    }

    pub fn query_limit(self) -> usize {
        self.query_limit
    }

    pub fn source_record_limit(self) -> usize {
        self.source_record_limit
    }

    pub fn precision_cache_limit(self) -> usize {
        self.precision_cache_limit
    }

    pub fn direction_cone_m(self) -> f64 {
        self.direction_cone_m
    }

    pub fn direction_cone_half_angle_deg(self) -> f64 {
        self.direction_cone_half_angle_deg
    }
}

/// Validated non-dateline-crossing WGS84 bounding box in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoBBox {
    south: f64,
    west: f64,
    north: f64,
    east: f64,
}

impl GeoBBox {
    /// Validate a bounding box with `-90 <= south < north <= 90` and `-180 <= west < east <= 180`.
    pub fn new(south: f64, west: f64, north: f64, east: f64) -> Result<Self, AlprMapError> {
        if !south.is_finite() || !west.is_finite() || !north.is_finite() || !east.is_finite() {
            return Err(AlprMapError::InvalidViewport(
                "coordinates must be finite numbers",
            ));
        }
        if south < -90.0 || north > 90.0 || south >= north {
            return Err(AlprMapError::InvalidViewport(
                "latitude bounds must satisfy -90 <= south < north <= 90",
            ));
        }
        if west < -180.0 || east > 180.0 || west >= east {
            return Err(AlprMapError::InvalidViewport(
                "longitude bounds must satisfy -180 <= west < east <= 180",
            ));
        }
        Ok(Self {
            south,
            west,
            north,
            east,
        })
    }

    pub fn south(self) -> f64 {
        self.south
    }

    pub fn west(self) -> f64 {
        self.west
    }

    pub fn north(self) -> f64 {
        self.north
    }

    pub fn east(self) -> f64 {
        self.east
    }

    pub fn lat_span(self) -> f64 {
        self.north - self.south
    }

    pub fn lon_span(self) -> f64 {
        self.east - self.west
    }

    pub fn center(self) -> (f64, f64) {
        (
            (self.south + self.north) * 0.5,
            (self.west + self.east) * 0.5,
        )
    }

    /// Snap the bounding box outward to a positive degree grid, clamped to valid WGS84 bounds.
    pub fn snap_outward(self, step_deg: f64) -> Result<Self, AlprMapError> {
        if !step_deg.is_finite() || step_deg <= 0.0 || step_deg > 10.0 {
            return Err(AlprMapError::InvalidPolicy(
                "snap step must be finite and in (0, 10]",
            ));
        }
        let down = |value: f64| (value / step_deg).floor() * step_deg;
        let up = |value: f64| (value / step_deg).ceil() * step_deg;
        Self::new(
            down(self.south).max(-90.0),
            down(self.west).max(-180.0),
            up(self.north).min(90.0),
            up(self.east).min(180.0),
        )
    }

    /// Whether `inner` lies entirely within `self`.
    pub fn contains_box(self, inner: Self) -> bool {
        inner.south >= self.south
            && inner.north <= self.north
            && inner.west >= self.west
            && inner.east <= self.east
    }

    /// Whether the point `(latitude_deg, longitude_deg)` lies within `self` (inclusive).
    pub fn contains_point(self, latitude_deg: f64, longitude_deg: f64) -> bool {
        latitude_deg.is_finite()
            && longitude_deg.is_finite()
            && latitude_deg >= self.south
            && latitude_deg <= self.north
            && longitude_deg >= self.west
            && longitude_deg <= self.east
    }

    /// Whether `self` and `other` share any overlapping area or boundary.
    pub fn intersects(self, other: Self) -> bool {
        !(self.east < other.west
            || self.west > other.east
            || self.north < other.south
            || self.south > other.north)
    }

    /// Whether this viewport is within the policy's maximum city-scale span.
    pub fn within_city_span(self, policy: AlprQueryPolicy) -> bool {
        let limit = policy.max_viewport_degrees + 2.0 * policy.query_snap_degrees + 1e-9;
        self.lat_span() <= limit && self.lon_span() <= limit
    }
}

fn lon_to_tile_x(lon_deg: f64, zoom: u8) -> u32 {
    let n = (1_u32 << zoom) as f64;
    let clamped = lon_deg.clamp(-180.0, 180.0);
    let raw = ((clamped + 180.0) / 360.0 * n).floor();
    (raw as i64).clamp(0, (1_i64 << zoom) - 1) as u32
}

fn lat_to_tile_y(lat_deg: f64, zoom: u8) -> u32 {
    let n = (1_u32 << zoom) as f64;
    let clamped = lat_deg.clamp(-WEB_MERCATOR_MAX_LAT_DEG, WEB_MERCATOR_MAX_LAT_DEG);
    let lat_rad = clamped.to_radians();
    let mercator = (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) * 0.5;
    let raw = (mercator * n).floor();
    (raw as i64).clamp(0, (1_i64 << zoom) - 1) as u32
}

/// Count how many Web Mercator XYZ tiles at `zoom` cover `bbox`, capped at `max_tiles + 1`.
pub fn tiles_for_bounds_count(bbox: GeoBBox, zoom: u8, max_tiles: usize) -> usize {
    if zoom > 22 {
        return max_tiles.saturating_add(1);
    }
    let x_min = lon_to_tile_x(bbox.west, zoom);
    let x_max = lon_to_tile_x(bbox.east, zoom);
    let y_min = lat_to_tile_y(bbox.north, zoom);
    let y_max = lat_to_tile_y(bbox.south, zoom);
    let width = (x_max.saturating_sub(x_min) as usize).saturating_add(1);
    let height = (y_max.saturating_sub(y_min) as usize).saturating_add(1);
    width
        .saturating_mul(height)
        .min(max_tiles.saturating_add(1))
}

/// Select the finest attributed extract zoom in `detail_min_zoom..=detail_max_zoom`
/// whose tile count for `bbox` is at most `detail_max_tiles`, or `None` if even
/// the coarsest detail zoom exceeds the tile budget.
pub fn alpr_detail_zoom(bbox: GeoBBox, policy: AlprQueryPolicy) -> Option<u8> {
    let mut zoom = policy.detail_max_zoom;
    loop {
        if tiles_for_bounds_count(bbox, zoom, policy.detail_max_tiles) <= policy.detail_max_tiles {
            return Some(zoom);
        }
        if zoom <= policy.detail_min_zoom {
            return None;
        }
        zoom = zoom.saturating_sub(1);
    }
}

/// Exponential backoff progression for unavailable source retries (`30 s` doubling to `240 s`).
pub fn alpr_retry_delay_ms(prev_delay_ms: Option<u64>) -> u64 {
    match prev_delay_ms {
        None | Some(0) => RETRY_MIN_MS,
        Some(prev) => prev.saturating_mul(2).clamp(RETRY_MIN_MS, RETRY_CEIL_MS),
    }
}

/// Whether an OpenStreetMap `surveillance:type` value denotes an ALPR camera.
/// Supports semicolon-delimited multi-values (`camera;ALPR`) and mixed case.
pub fn is_alpr_surveillance_type(raw: &str) -> bool {
    raw.split(';')
        .any(|part| part.trim().eq_ignore_ascii_case("ALPR"))
}

/// Parse an optional compass bearing in degrees clockwise from north (`[0.0, 360.0)`).
///
/// Blank or whitespace-only strings and non-numeric cardinal strings (`"N"`, `"NE"`)
/// return `None` so missing data never masquerades as a `0°` due-north measurement.
/// `360.0` normalizes to `0.0`.
pub fn normalize_direction_deg(raw: Option<&str>) -> Option<f64> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed: f64 = trimmed.parse().ok()?;
    if !parsed.is_finite() || !(0.0..=360.0).contains(&parsed) {
        return None;
    }
    Some(if parsed == 360.0 {
        0.0
    } else {
        parsed.rem_euclid(360.0)
    })
}

fn clean_tag(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// Explicit manufacturer classification for a mapped ALPR node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AlprManufacturerClass {
    /// Manufacturer, brand, or operator explicitly names Flock Safety.
    FlockSafety,
    /// Another explicit ALPR manufacturer is named (e.g., Motorola, Genetec, Leonardo).
    OtherNamed(String),
    /// Node is tagged as an ALPR camera, but no manufacturer/brand tag is recorded.
    Unspecified,
}

impl AlprManufacturerClass {
    pub fn short_label(&self) -> &str {
        match self {
            Self::FlockSafety => "Flock Safety",
            Self::OtherNamed(vendor) => vendor.as_str(),
            Self::Unspecified => "Unspecified ALPR vendor",
        }
    }

    pub fn is_flock_safety(&self) -> bool {
        matches!(self, Self::FlockSafety)
    }
}

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    let needle_bytes = needle.as_bytes();
    if needle_bytes.is_empty() {
        return true;
    }
    haystack
        .as_bytes()
        .windows(needle_bytes.len())
        .any(|window| window.eq_ignore_ascii_case(needle_bytes))
}

/// Classify a mapped ALPR node's manufacturer without inventing a vendor for untagged nodes.
pub fn classify_manufacturer(
    manufacturer: Option<&str>,
    brand: Option<&str>,
    operator: Option<&str>,
) -> (Option<String>, AlprManufacturerClass) {
    let resolved_vendor = clean_tag(manufacturer).or_else(|| clean_tag(brand));
    if let Some(ref vendor) = resolved_vendor {
        if contains_ascii_case_insensitive(vendor, "flock") {
            return (resolved_vendor, AlprManufacturerClass::FlockSafety);
        }
        return (
            resolved_vendor.clone(),
            AlprManufacturerClass::OtherNamed(vendor.clone()),
        );
    }
    if let Some(op) = operator
        && contains_ascii_case_insensitive(op, "flock safety")
    {
        return (
            Some("Flock Safety".to_owned()),
            AlprManufacturerClass::FlockSafety,
        );
    }
    (None, AlprManufacturerClass::Unspecified)
}

/// Reserved minimum OpenStreetMap node ID for synthetic OIA test fixtures (`>= 900_000_000_000`).
pub const SYNTHETIC_OSM_ID_FLOOR: u64 = 900_000_000_000;

/// Raw OpenStreetMap node input from Overpass JSON or an hourly vector-tile extract.
#[derive(Clone, Debug, PartialEq)]
pub struct RawOsmNode {
    pub element_type: String,
    pub osm_id: i64,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub coordinate_zoom: Option<u8>,
    pub surveillance_type: Option<String>,
    pub manufacturer: Option<String>,
    pub brand: Option<String>,
    pub operator: Option<String>,
    pub camera_type: Option<String>,
    pub surveillance_zone: Option<String>,
    pub camera_direction: Option<String>,
    pub direction: Option<String>,
    pub reference: Option<String>,
    pub check_date: Option<String>,
    pub survey_date: Option<String>,
    pub osm_timestamp: Option<String>,
    pub source: Option<String>,
    pub note: Option<String>,
    pub oia_synthetic: Option<String>,
    pub synthetic: bool,
}

/// Normalized, provenance-preserving ALPR directory camera record (`CAM-06`).
#[derive(Clone, Debug, PartialEq)]
pub struct AlprCameraRecord {
    pub id: String,
    pub osm_id: u64,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub coordinate_zoom: Option<u8>,
    pub manufacturer: Option<String>,
    pub manufacturer_class: AlprManufacturerClass,
    pub operator: Option<String>,
    pub camera_type: Option<String>,
    pub zone: Option<String>,
    pub direction_deg: Option<f64>,
    pub reference: Option<String>,
    /// Field verification date (`check_date` or `survey:date`), kept separate from `osm_timestamp`.
    pub last_verified: Option<String>,
    /// Upstream OSM edit/extract timestamp, never promoted to `last_verified`.
    pub osm_timestamp: Option<String>,
    pub source_tag: Option<String>,
    /// Preserved `note` tag (including `"SYNTHETIC FIXTURE - NOT OBSERVED"` on fixture records).
    pub note: Option<String>,
    /// Explicit synthetic marker carried from `synthetic: true`, `oia:synthetic=true`, or `SYNTHETIC_OSM_ID_FLOOR`.
    pub synthetic: bool,
}

impl AlprCameraRecord {
    /// Compact display badge identifier (`ALPR-XXXX`); selection identity always uses `self.id`.
    pub fn display_id(&self) -> String {
        let suffix = self.osm_id % 10_000;
        format!("ALPR-{suffix:04}")
    }

    /// Structured readout lines naming the actual source and supplied metadata only.
    pub fn label_details(&self, source_name: &str) -> Vec<String> {
        let mut details = Vec::new();
        if self.synthetic {
            details.push("SYNTHETIC FIXTURE - NOT OBSERVED".to_owned());
        } else if source_name == "OpenStreetMap" {
            details.push("OSM MAPPED".to_owned());
        } else {
            details.push(format!("Source: {source_name}"));
        }
        if let Some(direction) = self.direction_deg {
            details.push(format!("DIRECTION {}°", direction.round() as i64));
        }
        let mut equipment: Vec<String> = Vec::new();
        for candidate in [
            self.manufacturer.as_deref(),
            self.operator.as_deref(),
            self.camera_type.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            let upper = candidate.trim().to_ascii_uppercase();
            if !upper.is_empty() && !equipment.contains(&upper) {
                equipment.push(upper);
            }
        }
        if !equipment.is_empty() {
            details.push(equipment.join(" · "));
        }
        if !self.synthetic && source_name == "OpenStreetMap" {
            details.push("PUBLIC MAP DATA".to_owned());
        }
        details
    }
}

/// Normalize one raw OpenStreetMap node element into an [`AlprCameraRecord`].
/// Returns `None` for non-nodes, invalid coordinates/IDs, or nodes without an `ALPR` surveillance tag.
pub fn normalize_alpr_node(raw: &RawOsmNode) -> Option<AlprCameraRecord> {
    if raw.element_type.trim() != "node" || raw.osm_id <= 0 {
        return None;
    }
    if !raw.latitude_deg.is_finite()
        || raw.latitude_deg.abs() > 90.0
        || !raw.longitude_deg.is_finite()
        || raw.longitude_deg.abs() > 180.0
    {
        return None;
    }
    let surv_type = raw.surveillance_type.as_deref()?;
    if !is_alpr_surveillance_type(surv_type) {
        return None;
    }
    let osm_id = u64::try_from(raw.osm_id).ok()?;
    let (manufacturer, manufacturer_class) = classify_manufacturer(
        raw.manufacturer.as_deref(),
        raw.brand.as_deref(),
        raw.operator.as_deref(),
    );
    let direction_deg =
        normalize_direction_deg(raw.camera_direction.as_deref().or(raw.direction.as_deref()));
    let last_verified =
        clean_tag(raw.check_date.as_deref()).or_else(|| clean_tag(raw.survey_date.as_deref()));
    let note = clean_tag(raw.note.as_deref());
    let tag_synthetic = raw
        .oia_synthetic
        .as_deref()
        .is_some_and(|val| val.trim().eq_ignore_ascii_case("true"))
        || note
            .as_deref()
            .is_some_and(|n| contains_ascii_case_insensitive(n, "SYNTHETIC FIXTURE"));
    let synthetic = raw.synthetic || tag_synthetic || osm_id >= SYNTHETIC_OSM_ID_FLOOR;
    Some(AlprCameraRecord {
        id: format!("alpr:{osm_id}"),
        osm_id,
        latitude_deg: raw.latitude_deg,
        longitude_deg: raw.longitude_deg,
        coordinate_zoom: raw.coordinate_zoom,
        manufacturer,
        manufacturer_class,
        operator: clean_tag(raw.operator.as_deref()),
        camera_type: clean_tag(raw.camera_type.as_deref()),
        zone: clean_tag(raw.surveillance_zone.as_deref()),
        direction_deg,
        reference: clean_tag(raw.reference.as_deref()),
        last_verified,
        osm_timestamp: clean_tag(raw.osm_timestamp.as_deref()),
        source_tag: clean_tag(raw.source.as_deref()),
        note,
        synthetic,
    })
}

/// Validate a collection of normalized camera records for finite coordinates, valid bearings,
/// and unique non-empty IDs before committing to a view or store.
pub fn validate_alpr_records(records: &[AlprCameraRecord]) -> Result<(), AlprMapError> {
    let mut seen_ids = BTreeSet::new();
    for record in records {
        if record.id.trim().is_empty() {
            return Err(AlprMapError::InvalidRecord("record id must not be empty"));
        }
        if !seen_ids.insert(record.id.clone()) {
            return Err(AlprMapError::DuplicateRecordId(record.id.clone()));
        }
        if !record.latitude_deg.is_finite()
            || record.latitude_deg.abs() > 90.0
            || !record.longitude_deg.is_finite()
            || record.longitude_deg.abs() > 180.0
        {
            return Err(AlprMapError::InvalidRecord(
                "record coordinates must be finite and within WGS84 bounds",
            ));
        }
        if let Some(direction) = record.direction_deg
            && (!direction.is_finite() || !(0.0..360.0).contains(&direction))
        {
            return Err(AlprMapError::InvalidRecord(
                "direction_deg must be finite and in [0, 360)",
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct PrecisionEntry {
    id: String,
    zoom: u8,
    latitude_deg: f64,
    longitude_deg: f64,
}

/// Bounded coordinate precision cache so coarser zoom vector tiles (`z9`, `z10`)
/// never overwrite finer coordinates (`z12`) already observed for the same camera ID,
/// while still allowing non-coordinate tags to refresh.
#[derive(Clone, Debug)]
pub struct AlprPrecisionCache {
    limit: usize,
    entries: Vec<PrecisionEntry>,
}

impl AlprPrecisionCache {
    pub fn new(limit: usize) -> Result<Self, AlprMapError> {
        if limit == 0 {
            return Err(AlprMapError::InvalidPolicy(
                "precision cache limit must be greater than zero",
            ));
        }
        Ok(Self {
            limit,
            entries: Vec::new(),
        })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Apply precision retention for `record` observed at `zoom`.
    pub fn apply(&mut self, mut record: AlprCameraRecord, zoom: u8) -> AlprCameraRecord {
        if let Some(pos) = self.entries.iter().position(|entry| entry.id == record.id) {
            if let Some(existing) = self.entries.get(pos)
                && existing.zoom > zoom
            {
                record.latitude_deg = existing.latitude_deg;
                record.longitude_deg = existing.longitude_deg;
                record.coordinate_zoom = Some(existing.zoom);
                return record;
            }
            self.entries.remove(pos);
        }
        self.entries.push(PrecisionEntry {
            id: record.id.clone(),
            zoom,
            latitude_deg: record.latitude_deg,
            longitude_deg: record.longitude_deg,
        });
        while self.entries.len() > self.limit {
            self.entries.remove(0);
        }
        record.coordinate_zoom = Some(zoom);
        record
    }
}

/// Geometry provenance classification for map overlays.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryClass {
    /// Illustrative bearing wedge derived from a numeric compass tag; not a calibrated FOV footprint.
    IllustrativeBearingWedge,
}

/// Three-vertex illustrative directional wedge `[apex, left_corner, right_corner]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionWedge {
    pub geometry_class: GeometryClass,
    pub apex_lat_deg: f64,
    pub apex_lon_deg: f64,
    pub left_lat_deg: f64,
    pub left_lon_deg: f64,
    pub right_lat_deg: f64,
    pub right_lon_deg: f64,
    pub bearing_deg: f64,
    pub radius_m: f64,
    pub half_angle_deg: f64,
}

/// Great-circle destination point `(latitude_deg, longitude_deg)` from `(lat_deg, lon_deg)`
/// along `bearing_deg` (clockwise from true north) for `distance_m` on a sphere of `EARTH_MEAN_RADIUS_M`.
pub fn destination_point_deg(
    lat_deg: f64,
    lon_deg: f64,
    bearing_deg: f64,
    distance_m: f64,
) -> (f64, f64) {
    let angular_distance = distance_m / EARTH_MEAN_RADIUS_M;
    let bearing = bearing_deg.to_radians();
    let lat1 = lat_deg.to_radians();
    let lon1 = lon_deg.to_radians();
    let sin_lat2 =
        lat1.sin() * angular_distance.cos() + lat1.cos() * angular_distance.sin() * bearing.cos();
    let lat2 = sin_lat2.clamp(-1.0, 1.0).asin();
    let lon2 = lon1
        + (bearing.sin() * angular_distance.sin() * lat1.cos())
            .atan2(angular_distance.cos() - lat1.sin() * lat2.sin());
    (lat2.to_degrees(), lon2.to_degrees())
}

/// Compute the illustrative 3-point bearing wedge for a camera with a known numeric `direction_deg`.
pub fn direction_wedge_polygon(
    record: &AlprCameraRecord,
    policy: AlprQueryPolicy,
) -> Option<DirectionWedge> {
    let bearing_deg = record.direction_deg?;
    if !bearing_deg.is_finite() {
        return None;
    }
    let radius_m = policy.direction_cone_m;
    let half_angle_deg = policy.direction_cone_half_angle_deg;
    let (left_lat_deg, left_lon_deg) = destination_point_deg(
        record.latitude_deg,
        record.longitude_deg,
        bearing_deg - half_angle_deg,
        radius_m,
    );
    let (right_lat_deg, right_lon_deg) = destination_point_deg(
        record.latitude_deg,
        record.longitude_deg,
        bearing_deg + half_angle_deg,
        radius_m,
    );
    Some(DirectionWedge {
        geometry_class: GeometryClass::IllustrativeBearingWedge,
        apex_lat_deg: record.latitude_deg,
        apex_lon_deg: record.longitude_deg,
        left_lat_deg,
        left_lon_deg,
        right_lat_deg,
        right_lon_deg,
        bearing_deg,
        radius_m,
        half_angle_deg,
    })
}

/// Build the bounded Overpass QL query for ALPR nodes in `bbox`.
/// Uses a case-insensitive semicolon-aware regex so `surveillance:type="camera;ALPR"` is retained.
pub fn build_overpass_query(bbox: GeoBBox, query_limit: usize) -> String {
    format!(
        "[out:json][timeout:20];node[\"man_made\"=\"surveillance\"][\"surveillance:type\"~\"(^|;)\\\\s*ALPR\\\\s*(;|$)\",i]({},{},{},{});out body {query_limit};",
        bbox.south, bbox.west, bbox.north, bbox.east
    )
}

fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Build an escaped HTML attribution badge from plain text and a validated public `https://` URL.
pub fn alpr_credit_markup(text: &str, href: &str) -> Result<Option<String>, AlprMapError> {
    let trimmed_text = text.trim();
    let trimmed_href = href.trim();
    if trimmed_text.is_empty() || trimmed_href.is_empty() {
        return Ok(None);
    }
    let Some(rest) = trimmed_href.strip_prefix("https://") else {
        return Err(AlprMapError::InvalidAttributionHref(
            "camera attribution requires a public https:// link",
        ));
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty()
        || authority.contains('@')
        || trimmed_href.chars().any(char::is_whitespace)
    {
        return Err(AlprMapError::InvalidAttributionHref(
            "camera attribution URL must have a host and no embedded credentials or whitespace",
        ));
    }
    Ok(Some(format!(
        "<span class=\"oia-alpr-credit\">ALPR: <a href=\"{}\" target=\"_blank\" rel=\"noopener\">{}</a></span>",
        escape_html(trimmed_href),
        escape_html(trimmed_text)
    )))
}

/// Local view filter applied to mapped ALPR records (`CASE-02`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlprLocalFilter {
    /// Show all mapped ALPR cameras regardless of manufacturer tag.
    AllMappedAlpr,
    /// Show only cameras explicitly attributed to Flock Safety.
    FlockSafetyOnly,
    /// Show only cameras attributed to another named ALPR vendor.
    OtherNamedOnly,
    /// Show only ALPR cameras whose manufacturer tag is absent.
    UnspecifiedOnly,
    /// Show only cameras carrying a numeric compass bearing.
    WithBearingOnly,
}

impl AlprLocalFilter {
    pub fn matches(self, record: &AlprCameraRecord) -> bool {
        match self {
            Self::AllMappedAlpr => true,
            Self::FlockSafetyOnly => record.manufacturer_class.is_flock_safety(),
            Self::OtherNamedOnly => {
                matches!(
                    record.manufacturer_class,
                    AlprManufacturerClass::OtherNamed(_)
                )
            }
            Self::UnspecifiedOnly => {
                matches!(
                    record.manufacturer_class,
                    AlprManufacturerClass::Unspecified
                )
            }
            Self::WithBearingOnly => record.direction_deg.is_some(),
        }
    }

    pub fn is_filtered(self) -> bool {
        !matches!(self, Self::AllMappedAlpr)
    }

    pub fn chip_label(self) -> &'static str {
        match self {
            Self::AllMappedAlpr => "All mapped ALPR",
            Self::FlockSafetyOnly => "Local filter: Manufacturer = Flock Safety",
            Self::OtherNamedOnly => "Local filter: Manufacturer = Other named ALPR",
            Self::UnspecifiedOnly => "Local filter: Manufacturer = Unspecified",
            Self::WithBearingOnly => "Local filter: Bearing recorded only",
        }
    }
}

/// Coverage outcome for a bounded ALPR viewport query (`GEO-02`, `CAM-06`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlprCoverageStatus {
    /// Viewport exceeds city-scale degree or tile limits; user must zoom in before querying.
    ZoomRequired,
    /// Viewport lies outside the configured regional extract bounds (e.g., outside US/Canada).
    NoCoverage,
    /// Viewport is covered and returned zero matching records.
    Empty,
    /// All matching records in the viewport were returned within the configured limit.
    Complete,
    /// Matching records exceeded `query_limit`; nearest records to viewport center were retained.
    Saturated {
        total_matching: usize,
        retained: usize,
        omitted: usize,
    },
}

/// Breakdown of all normalized ALPR nodes inside the viewport before local filtering.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ManufacturerBreakdown {
    pub total_in_viewport: usize,
    pub flock_safety: usize,
    pub other_named: usize,
    pub unspecified: usize,
    pub with_bearing: usize,
    pub verified_date_present: usize,
}

/// Complete, provenance-annotated result of a bounded viewport query.
#[derive(Clone, Debug, PartialEq)]
pub struct AlprQuerySnapshot {
    pub requested_bbox: GeoBBox,
    pub snapped_bbox: GeoBBox,
    pub detail_zoom: Option<u8>,
    pub coverage_status: AlprCoverageStatus,
    pub breakdown: ManufacturerBreakdown,
    pub active_filter: AlprLocalFilter,
    pub local_filter_omitted: usize,
    pub records: Vec<AlprCameraRecord>,
}

/// Evaluate a bounded ALPR viewport query over raw OSM nodes with cooperative stop checks.
pub fn evaluate_viewport_query<F>(
    requested_bbox: GeoBBox,
    extract_bounds: Option<GeoBBox>,
    raw_nodes: &[RawOsmNode],
    filter: AlprLocalFilter,
    policy: AlprQueryPolicy,
    mut check_control: F,
) -> Result<AlprQuerySnapshot, AlprMapError>
where
    F: FnMut() -> Option<StopReason>,
{
    if let Some(reason) = check_control() {
        return Err(AlprMapError::Stopped(reason));
    }
    let snapped_bbox = requested_bbox.snap_outward(policy.query_snap_degrees)?;
    if !requested_bbox.within_city_span(policy) {
        return Ok(AlprQuerySnapshot {
            requested_bbox,
            snapped_bbox,
            detail_zoom: None,
            coverage_status: AlprCoverageStatus::ZoomRequired,
            breakdown: ManufacturerBreakdown::default(),
            active_filter: filter,
            local_filter_omitted: 0,
            records: Vec::new(),
        });
    }
    if let Some(bounds) = extract_bounds
        && !requested_bbox.intersects(bounds)
    {
        return Ok(AlprQuerySnapshot {
            requested_bbox,
            snapped_bbox,
            detail_zoom: alpr_detail_zoom(requested_bbox, policy),
            coverage_status: AlprCoverageStatus::NoCoverage,
            breakdown: ManufacturerBreakdown::default(),
            active_filter: filter,
            local_filter_omitted: 0,
            records: Vec::new(),
        });
    }
    let Some(detail_zoom) = alpr_detail_zoom(requested_bbox, policy) else {
        return Ok(AlprQuerySnapshot {
            requested_bbox,
            snapped_bbox,
            detail_zoom: None,
            coverage_status: AlprCoverageStatus::ZoomRequired,
            breakdown: ManufacturerBreakdown::default(),
            active_filter: filter,
            local_filter_omitted: 0,
            records: Vec::new(),
        });
    };

    let mut seen_ids = BTreeSet::new();
    let mut in_viewport = Vec::new();
    for raw in raw_nodes {
        if let Some(reason) = check_control() {
            return Err(AlprMapError::Stopped(reason));
        }
        let Some(record) = normalize_alpr_node(raw) else {
            continue;
        };
        if !requested_bbox.contains_point(record.latitude_deg, record.longitude_deg) {
            continue;
        }
        if !seen_ids.insert(record.id.clone()) {
            continue;
        }
        if in_viewport.try_reserve(1).is_err() {
            return Err(AlprMapError::AllocationFailed);
        }
        in_viewport.push(record);
    }

    let mut breakdown = ManufacturerBreakdown {
        total_in_viewport: in_viewport.len(),
        ..ManufacturerBreakdown::default()
    };
    for record in &in_viewport {
        match record.manufacturer_class {
            AlprManufacturerClass::FlockSafety => {
                breakdown.flock_safety = breakdown.flock_safety.saturating_add(1);
            }
            AlprManufacturerClass::OtherNamed(_) => {
                breakdown.other_named = breakdown.other_named.saturating_add(1);
            }
            AlprManufacturerClass::Unspecified => {
                breakdown.unspecified = breakdown.unspecified.saturating_add(1);
            }
        }
        if record.direction_deg.is_some() {
            breakdown.with_bearing = breakdown.with_bearing.saturating_add(1);
        }
        if record.last_verified.is_some() {
            breakdown.verified_date_present = breakdown.verified_date_present.saturating_add(1);
        }
    }

    let mut matching: Vec<AlprCameraRecord> = in_viewport
        .into_iter()
        .filter(|record| filter.matches(record))
        .collect();
    let total_matching = matching.len();
    let local_filter_omitted = breakdown.total_in_viewport.saturating_sub(total_matching);

    let (center_lat, center_lon) = requested_bbox.center();
    let lon_scale = center_lat.to_radians().cos();
    matching.sort_by(|a, b| {
        let da = (a.latitude_deg - center_lat).powi(2)
            + ((a.longitude_deg - center_lon) * lon_scale).powi(2);
        let db = (b.latitude_deg - center_lat).powi(2)
            + ((b.longitude_deg - center_lon) * lon_scale).powi(2);
        da.total_cmp(&db).then_with(|| a.osm_id.cmp(&b.osm_id))
    });

    let coverage_status = if total_matching == 0 {
        AlprCoverageStatus::Empty
    } else if total_matching > policy.query_limit {
        let retained = policy.query_limit;
        matching.truncate(retained);
        AlprCoverageStatus::Saturated {
            total_matching,
            retained,
            omitted: total_matching.saturating_sub(retained),
        }
    } else {
        AlprCoverageStatus::Complete
    };

    if let Some(reason) = check_control() {
        return Err(AlprMapError::Stopped(reason));
    }

    Ok(AlprQuerySnapshot {
        requested_bbox,
        snapped_bbox,
        detail_zoom: Some(detail_zoom),
        coverage_status,
        breakdown,
        active_filter: filter,
        local_filter_omitted,
        records: matching,
    })
}
