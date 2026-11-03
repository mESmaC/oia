//! Linked geographic query envelopes, historical time/source qualifications,
//! reversible source-aware event clustering, discrete footprint mosaics, and
//! shared cross-view workspace navigation (`GEO-01`..=`GEO-04`, `CASE-02`,
//! `UX-02`, `A04`, `#23`).
//!
//! ## Scope and status (`AGENTS.md` §3, §4)
//!
//! - **Implemented (in-memory domain/query kernel):**
//!   - [`GeoPointWgs84`], [`GeoBoundingBoxWgs84`] (including antimeridian-crossing
//!     intervals where `west_lon_deg > east_lon_deg`), and [`GeoLocation`]
//!     distinguishing surveyed points, bounded regions, and explicitly labeled
//!     [`GeoLocation::ApproximateGazetteerCentroid`] records (`GEO-01`, `A04`).
//!   - [`ReportedEventTime`], [`ReportedTimePrecision`], and
//!     [`EarliestKnownSourceQualification`] preserving verbatim local time text,
//!     timezone/offset ambiguity, calendar-date-only records (which never
//!     masquerade as midnight UTC timestamps), and earliest-known-source
//!     qualifications for historical accounts (`GEO-01`, `docs/architecture.md` §3.1).
//!   - [`GeoCoverageEnvelope`] and [`ModalityCoverageRecord`] tracking per-modality
//!     coverage, stale snapshots, missing history, and track interpolation basis
//!     while preventing missing feeds or unrecorded intervals from being promoted
//!     to clear-sky or empty-scene conclusions (`GEO-02`, `GEO-05`, `A04`).
//!   - [`EventClusterLedger`] providing deterministic proximity/time/property/lineage
//!     clustering, conflict disclosure ([`ClusterConflict`]), four strictly distinct
//!     counts ([`DistinctEvidenceCounts`]: articles, witness accounts, independent
//!     sources after lineage/hash/source collapse, and proposed events, keeping
//!     [`AccountOrigin::Simulated`] items excluded from observed counts), and
//!     reversible split/merge/revert operations with immutable [`VersionId`]s
//!     (`GEO-03`, `A04`).
//!   - [`FootprintMosaic`] and [`CrossViewAlignment`] representing camera viewing
//!     cones and satellite footprints as a discrete mosaic (never a continuous
//!     seamless reconstruction) and preserving calibration and clock-alignment
//!     assumptions (`GEO-04`, `A04`).
//!   - [`SharedWorkspaceContext`] preserving investigation, area, time, selected
//!     event/evidence, local filter divergence disclosures, and route-back /
//!     route-forward navigation history across workspace views (`CASE-02`, `UX-02`).
//! - **Remaining open on `#23`:**
//!   - Interactive 3D globe/canvas UI rendering (`#16`), SQLite spatial index
//!     persistence (`#11`, `#14`), and live external provider adapters (`#24`–`#26`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

/// Mean Earth radius in meters (WGS84 spherical approximation for proximity bounds).
pub const EARTH_MEAN_RADIUS_M: f64 = 6_371_008.8;
/// Maximum allowed uncertainty or range radius in meters (`20,000` km).
pub const MAX_DISTANCE_M: f64 = 20_000_000.0;
/// Maximum UTF-8 byte length for labels, notes, and rationales.
pub const DEFAULT_MAX_TEXT_BYTES: usize = 4_096;
/// Default maximum evidence items in a single [`EventClusterLedger`].
pub const DEFAULT_MAX_EVIDENCE_ITEMS: usize = 1_024;
/// Default maximum active clusters in a single [`EventClusterLedger`].
pub const DEFAULT_MAX_CLUSTERS: usize = 512;
/// Default maximum retained split/merge/revert history entries.
pub const DEFAULT_MAX_DECISION_HISTORY: usize = 512;
/// Default maximum mosaic tiles in a [`FootprintMosaic`].
pub const DEFAULT_MAX_MOSAIC_TILES: usize = 256;
/// Default maximum navigation history entries in [`SharedWorkspaceContext`].
pub const DEFAULT_MAX_NAVIGATION_STEPS: usize = 128;

/// Resource limits for geographic query, clustering, mosaic, and navigation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeoWorkspaceLimits {
    pub max_text_bytes: usize,
    pub max_evidence_items: usize,
    pub max_clusters: usize,
    pub max_decision_history: usize,
    pub max_mosaic_tiles: usize,
    pub max_navigation_steps: usize,
}

impl GeoWorkspaceLimits {
    /// Validates that all configured resource bounds are non-zero.
    pub const fn new(
        max_text_bytes: usize,
        max_evidence_items: usize,
        max_clusters: usize,
        max_decision_history: usize,
        max_mosaic_tiles: usize,
        max_navigation_steps: usize,
    ) -> Result<Self, GeoWorkspaceError> {
        if max_text_bytes == 0
            || max_evidence_items == 0
            || max_clusters == 0
            || max_decision_history == 0
            || max_mosaic_tiles == 0
            || max_navigation_steps == 0
        {
            return Err(GeoWorkspaceError::ZeroResourceLimit);
        }
        Ok(Self {
            max_text_bytes,
            max_evidence_items,
            max_clusters,
            max_decision_history,
            max_mosaic_tiles,
            max_navigation_steps,
        })
    }
}

impl Default for GeoWorkspaceLimits {
    fn default() -> Self {
        Self {
            max_text_bytes: DEFAULT_MAX_TEXT_BYTES,
            max_evidence_items: DEFAULT_MAX_EVIDENCE_ITEMS,
            max_clusters: DEFAULT_MAX_CLUSTERS,
            max_decision_history: DEFAULT_MAX_DECISION_HISTORY,
            max_mosaic_tiles: DEFAULT_MAX_MOSAIC_TILES,
            max_navigation_steps: DEFAULT_MAX_NAVIGATION_STEPS,
        }
    }
}

/// Errors returned by geographic query, clustering, mosaic, and workspace operations.
#[derive(Debug, Clone, PartialEq)]
pub enum GeoWorkspaceError {
    ZeroResourceLimit,
    EmptyTextField(&'static str),
    TextFieldTooLong {
        field: &'static str,
        len: usize,
        max: usize,
    },
    ControlCharacterInText(&'static str),
    InvalidLatitude(f64),
    InvalidLongitude(f64),
    InvertedLatitudeBounds {
        south_lat_deg: f64,
        north_lat_deg: f64,
    },
    InvalidDistanceOrRadius {
        field: &'static str,
        value: f64,
    },
    InvalidUtcOffsetMinutes(i16),
    InvalidCalendarDate {
        year: i32,
        month: u8,
        day: u8,
    },
    InvalidTimeInterval {
        start_utc_ms: i64,
        end_utc_ms: i64,
    },
    InvalidAzimuthDeg(f64),
    InvalidFieldOfViewDeg(f64),
    InvalidRangeBounds {
        near_range_m: f64,
        far_range_m: f64,
    },
    DuplicateEvidenceItem(OpaqueId),
    UnknownEvidenceItem(OpaqueId),
    DuplicateClusterId(OpaqueId),
    UnknownClusterId(OpaqueId),
    DuplicateVersionId(VersionId),
    DuplicateDecisionId(OpaqueId),
    UnknownDecisionId(OpaqueId),
    DecisionAlreadyReverted(OpaqueId),
    DecisionNoLongerCurrent {
        decision_id: OpaqueId,
        missing_or_modified_cluster: OpaqueId,
    },
    InvalidClusterSplitPartition {
        cluster_id: OpaqueId,
        reason: &'static str,
    },
    SelfLineageReference(OpaqueId),
    ResourceLimitExceeded(&'static str),
    DuplicateMosaicTile(OpaqueId),
    UnknownMosaicTile(OpaqueId),
    EmptyNavigationRouteBack,
    EmptyNavigationRouteForward,
}

impl fmt::Display for GeoWorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroResourceLimit => f.write_str("all workspace resource limits must be >= 1"),
            Self::EmptyTextField(field) => write!(f, "{field} must not be empty"),
            Self::TextFieldTooLong { field, len, max } => {
                write!(f, "{field} byte length {len} exceeds maximum {max}")
            }
            Self::ControlCharacterInText(field) => {
                write!(f, "{field} contains disallowed control characters")
            }
            Self::InvalidLatitude(lat) => {
                write!(f, "latitude {lat} must be finite and within [-90, 90]")
            }
            Self::InvalidLongitude(lon) => {
                write!(f, "longitude {lon} must be finite and within [-180, 180]")
            }
            Self::InvertedLatitudeBounds {
                south_lat_deg,
                north_lat_deg,
            } => write!(
                f,
                "south latitude {south_lat_deg} exceeds north latitude {north_lat_deg}"
            ),
            Self::InvalidDistanceOrRadius { field, value } => {
                write!(
                    f,
                    "{field} ({value}) must be finite and within (0, {MAX_DISTANCE_M}]"
                )
            }
            Self::InvalidUtcOffsetMinutes(offset) => {
                write!(f, "UTC offset {offset} minutes is outside [-840, 840]")
            }
            Self::InvalidCalendarDate { year, month, day } => {
                write!(f, "invalid calendar date {year:04}-{month:02}-{day:02}")
            }
            Self::InvalidTimeInterval {
                start_utc_ms,
                end_utc_ms,
            } => write!(
                f,
                "time interval start ({start_utc_ms}) must be strictly less than end ({end_utc_ms})"
            ),
            Self::InvalidAzimuthDeg(az) => {
                write!(f, "azimuth {az} must be finite and within [0, 360)")
            }
            Self::InvalidFieldOfViewDeg(fov) => {
                write!(f, "field of view {fov} must be finite and within (0, 360]")
            }
            Self::InvalidRangeBounds {
                near_range_m,
                far_range_m,
            } => write!(
                f,
                "viewing cone range [{near_range_m}, {far_range_m}] must satisfy 0 <= near < far <= {MAX_DISTANCE_M}"
            ),
            Self::DuplicateEvidenceItem(id) => write!(f, "duplicate evidence item ID {id}"),
            Self::UnknownEvidenceItem(id) => write!(f, "unknown evidence item ID {id}"),
            Self::DuplicateClusterId(id) => write!(f, "duplicate cluster ID {id}"),
            Self::UnknownClusterId(id) => write!(f, "unknown cluster ID {id}"),
            Self::DuplicateVersionId(v) => write!(f, "version ID {v} was already used"),
            Self::DuplicateDecisionId(id) => write!(f, "decision ID {id} was already used"),
            Self::UnknownDecisionId(id) => write!(f, "unknown decision ID {id}"),
            Self::DecisionAlreadyReverted(id) => write!(f, "decision {id} was already reverted"),
            Self::DecisionNoLongerCurrent {
                decision_id,
                missing_or_modified_cluster,
            } => write!(
                f,
                "decision {decision_id} cannot be reverted directly because cluster {missing_or_modified_cluster} was modified by a later decision"
            ),
            Self::InvalidClusterSplitPartition { cluster_id, reason } => {
                write!(
                    f,
                    "invalid split partition for cluster {cluster_id}: {reason}"
                )
            }
            Self::SelfLineageReference(id) => {
                write!(f, "evidence item {id} cannot reference itself in lineage")
            }
            Self::ResourceLimitExceeded(limit) => write!(f, "resource limit exceeded: {limit}"),
            Self::DuplicateMosaicTile(id) => write!(f, "duplicate mosaic tile ID {id}"),
            Self::UnknownMosaicTile(id) => write!(f, "unknown mosaic tile ID {id}"),
            Self::EmptyNavigationRouteBack => {
                f.write_str("no previous navigation entry in route_back")
            }
            Self::EmptyNavigationRouteForward => {
                f.write_str("no forward navigation entry in route_forward")
            }
        }
    }
}

impl std::error::Error for GeoWorkspaceError {}

fn validate_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), GeoWorkspaceError> {
    if value.trim().is_empty() {
        return Err(GeoWorkspaceError::EmptyTextField(field));
    }
    if value.len() > max_bytes {
        return Err(GeoWorkspaceError::TextFieldTooLong {
            field,
            len: value.len(),
            max: max_bytes,
        });
    }
    if value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
    {
        return Err(GeoWorkspaceError::ControlCharacterInText(field));
    }
    Ok(())
}

fn validate_lat(lat_deg: f64) -> Result<f64, GeoWorkspaceError> {
    if !lat_deg.is_finite() || !(-90.0..=90.0).contains(&lat_deg) {
        return Err(GeoWorkspaceError::InvalidLatitude(lat_deg));
    }
    Ok(lat_deg)
}

fn validate_lon(lon_deg: f64) -> Result<f64, GeoWorkspaceError> {
    if !lon_deg.is_finite() || !(-180.0..=180.0).contains(&lon_deg) {
        return Err(GeoWorkspaceError::InvalidLongitude(lon_deg));
    }
    Ok(lon_deg)
}

fn validate_positive_distance(field: &'static str, value: f64) -> Result<f64, GeoWorkspaceError> {
    if !value.is_finite() || value <= 0.0 || value > MAX_DISTANCE_M {
        return Err(GeoWorkspaceError::InvalidDistanceOrRadius { field, value });
    }
    Ok(value)
}

/// Validated WGS84 latitude/longitude point (`GEO-01`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPointWgs84 {
    lat_deg: f64,
    lon_deg: f64,
}

impl GeoPointWgs84 {
    /// Validates finite latitude in `[-90, 90]` and longitude in `[-180, 180]`.
    pub fn new(lat_deg: f64, lon_deg: f64) -> Result<Self, GeoWorkspaceError> {
        Ok(Self {
            lat_deg: validate_lat(lat_deg)?,
            lon_deg: validate_lon(lon_deg)?,
        })
    }

    #[must_use]
    pub const fn lat_deg(self) -> f64 {
        self.lat_deg
    }

    #[must_use]
    pub const fn lon_deg(self) -> f64 {
        self.lon_deg
    }

    /// Great-circle haversine distance in meters between two WGS84 points.
    #[must_use]
    pub fn haversine_distance_m(self, other: Self) -> f64 {
        let lat1 = self.lat_deg.to_radians();
        let lat2 = other.lat_deg.to_radians();
        let dlat = (other.lat_deg - self.lat_deg).to_radians();
        let dlon = (other.lon_deg - self.lon_deg).to_radians();
        let sin_dlat = (dlat * 0.5).sin();
        let sin_dlon = (dlon * 0.5).sin();
        let a = sin_dlat * sin_dlat + lat1.cos() * lat2.cos() * sin_dlon * sin_dlon;
        let clamped = a.clamp(0.0, 1.0);
        2.0 * EARTH_MEAN_RADIUS_M * clamped.sqrt().asin()
    }

    /// Initial forward azimuth in degrees `[0, 360)` from `self` to `target`.
    #[must_use]
    pub fn initial_bearing_deg(self, target: Self) -> f64 {
        let lat1 = self.lat_deg.to_radians();
        let lat2 = target.lat_deg.to_radians();
        let dlon = (target.lon_deg - self.lon_deg).to_radians();
        let y = dlon.sin() * lat2.cos();
        let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
        (y.atan2(x).to_degrees() + 360.0).rem_euclid(360.0)
    }
}

/// WGS84 geographic bounding box with explicit antimeridian-crossing support (`GEO-01`, `A04`).
///
/// When `west_lon_deg > east_lon_deg` (for example `172.0` to `-172.0`), the box
/// crosses the `±180°` antimeridian (`crosses_antimeridian() == true`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoBoundingBoxWgs84 {
    south_lat_deg: f64,
    north_lat_deg: f64,
    west_lon_deg: f64,
    east_lon_deg: f64,
}

impl GeoBoundingBoxWgs84 {
    /// Validates latitude bounds (`south <= north`) and longitude bounds in `[-180, 180]`.
    pub fn new(
        south_lat_deg: f64,
        north_lat_deg: f64,
        west_lon_deg: f64,
        east_lon_deg: f64,
    ) -> Result<Self, GeoWorkspaceError> {
        let south = validate_lat(south_lat_deg)?;
        let north = validate_lat(north_lat_deg)?;
        if south > north {
            return Err(GeoWorkspaceError::InvertedLatitudeBounds {
                south_lat_deg: south,
                north_lat_deg: north,
            });
        }
        Ok(Self {
            south_lat_deg: south,
            north_lat_deg: north,
            west_lon_deg: validate_lon(west_lon_deg)?,
            east_lon_deg: validate_lon(east_lon_deg)?,
        })
    }

    #[must_use]
    pub const fn south_lat_deg(self) -> f64 {
        self.south_lat_deg
    }

    #[must_use]
    pub const fn north_lat_deg(self) -> f64 {
        self.north_lat_deg
    }

    #[must_use]
    pub const fn west_lon_deg(self) -> f64 {
        self.west_lon_deg
    }

    #[must_use]
    pub const fn east_lon_deg(self) -> f64 {
        self.east_lon_deg
    }

    /// Returns `true` when the longitude interval wraps across the `±180°` dateline.
    #[must_use]
    pub fn crosses_antimeridian(self) -> bool {
        self.west_lon_deg > self.east_lon_deg
    }

    /// Returns `true` if `point` lies within this bounding box, accounting for dateline wrapping.
    #[must_use]
    pub fn contains_point(self, point: GeoPointWgs84) -> bool {
        if point.lat_deg < self.south_lat_deg || point.lat_deg > self.north_lat_deg {
            return false;
        }
        if self.crosses_antimeridian() {
            point.lon_deg >= self.west_lon_deg || point.lon_deg <= self.east_lon_deg
        } else {
            point.lon_deg >= self.west_lon_deg && point.lon_deg <= self.east_lon_deg
        }
    }

    /// Returns `true` if `other` overlaps this bounding box in both latitude and longitude.
    #[must_use]
    pub fn intersects_box(self, other: Self) -> bool {
        let lat_overlap =
            self.south_lat_deg <= other.north_lat_deg && other.south_lat_deg <= self.north_lat_deg;
        if !lat_overlap {
            return false;
        }
        let lon_segments = |b: Self| -> [(f64, f64); 2] {
            if b.crosses_antimeridian() {
                [(b.west_lon_deg, 180.0), (-180.0, b.east_lon_deg)]
            } else {
                [(b.west_lon_deg, b.east_lon_deg), (f64::NAN, f64::NAN)]
            }
        };
        let segs_a = lon_segments(self);
        let segs_b = lon_segments(other);
        for (a_min, a_max) in segs_a {
            if !a_min.is_finite() {
                continue;
            }
            for (b_min, b_max) in segs_b {
                if !b_min.is_finite() {
                    continue;
                }
                if a_min <= b_max && b_min <= a_max {
                    return true;
                }
            }
        }
        false
    }

    /// Nominal center point of the bounding box (wrapping across `±180°` when needed).
    #[must_use]
    pub fn center_point(self) -> GeoPointWgs84 {
        let lat_deg = 0.5 * (self.south_lat_deg + self.north_lat_deg);
        let lon_deg = if self.crosses_antimeridian() {
            let span = (180.0 - self.west_lon_deg) + (self.east_lon_deg + 180.0);
            let raw = self.west_lon_deg + 0.5 * span;
            if raw > 180.0 { raw - 360.0 } else { raw }
        } else {
            0.5 * (self.west_lon_deg + self.east_lon_deg)
        };
        GeoPointWgs84 { lat_deg, lon_deg }
    }
}

/// Spatial geometry with explicit precision and gazetteer-centroid qualification (`GEO-01`).
#[derive(Debug, Clone, PartialEq)]
pub enum GeoLocation {
    /// Surveyed or instrument-measured point; `None` uncertainty means unknown, never zero.
    ExactSurveyedPoint {
        point: GeoPointWgs84,
        horizontal_uncertainty_m: Option<f64>,
    },
    /// Bounded geographic region or search envelope.
    BoundedArea { bounds: GeoBoundingBoxWgs84 },
    /// Gazetteer or named-place centroid; always carries a positive uncertainty radius
    /// so it never masquerades as an exact surveyed coordinate (`docs/architecture.md` §3.2).
    ApproximateGazetteerCentroid {
        place_label: String,
        nominal_centroid: GeoPointWgs84,
        radius_uncertainty_m: f64,
    },
}

impl GeoLocation {
    /// Constructs a surveyed point with optional positive horizontal uncertainty.
    pub fn surveyed_point(
        point: GeoPointWgs84,
        horizontal_uncertainty_m: Option<f64>,
    ) -> Result<Self, GeoWorkspaceError> {
        if let Some(u) = horizontal_uncertainty_m {
            validate_positive_distance("horizontal_uncertainty_m", u)?;
        }
        Ok(Self::ExactSurveyedPoint {
            point,
            horizontal_uncertainty_m,
        })
    }

    /// Constructs an explicitly labeled approximate gazetteer centroid.
    pub fn gazetteer_centroid(
        place_label: impl Into<String>,
        nominal_centroid: GeoPointWgs84,
        radius_uncertainty_m: f64,
        max_text_bytes: usize,
    ) -> Result<Self, GeoWorkspaceError> {
        let place_label = place_label.into();
        validate_text("place_label", &place_label, max_text_bytes)?;
        let radius_uncertainty_m =
            validate_positive_distance("radius_uncertainty_m", radius_uncertainty_m)?;
        Ok(Self::ApproximateGazetteerCentroid {
            place_label,
            nominal_centroid,
            radius_uncertainty_m,
        })
    }

    /// Representative point used for coarse distance checks (preserving `is_approximate()`).
    #[must_use]
    pub fn representative_point(&self) -> GeoPointWgs84 {
        match self {
            Self::ExactSurveyedPoint { point, .. } => *point,
            Self::BoundedArea { bounds } => bounds.center_point(),
            Self::ApproximateGazetteerCentroid {
                nominal_centroid, ..
            } => *nominal_centroid,
        }
    }

    /// Returns `true` if this location is an approximate gazetteer centroid or bounded area.
    #[must_use]
    pub const fn is_approximate(&self) -> bool {
        !matches!(self, Self::ExactSurveyedPoint { .. })
    }
}

/// Qualification required when a historical account's earliest available record
/// postdates the reported event (`GEO-01`, `A04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EarliestKnownSourceQualification {
    pub earliest_source_id: OpaqueId,
    pub earliest_source_version: VersionId,
    pub earliest_publication_date_text: String,
    pub gap_after_reported_event_note: String,
}

impl EarliestKnownSourceQualification {
    pub fn new(
        earliest_source_id: OpaqueId,
        earliest_source_version: VersionId,
        earliest_publication_date_text: impl Into<String>,
        gap_after_reported_event_note: impl Into<String>,
        max_text_bytes: usize,
    ) -> Result<Self, GeoWorkspaceError> {
        let earliest_publication_date_text = earliest_publication_date_text.into();
        let gap_after_reported_event_note = gap_after_reported_event_note.into();
        validate_text(
            "earliest_publication_date_text",
            &earliest_publication_date_text,
            max_text_bytes,
        )?;
        validate_text(
            "gap_after_reported_event_note",
            &gap_after_reported_event_note,
            max_text_bytes,
        )?;
        Ok(Self {
            earliest_source_id,
            earliest_source_version,
            earliest_publication_date_text,
            gap_after_reported_event_note,
        })
    }
}

/// Temporal precision of a reported event (`GEO-01`, `docs/architecture.md` §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportedTimePrecision {
    /// Instrument or synchronized timestamp in milliseconds since Unix epoch.
    ExactInstant { utc_ms: i64 },
    /// Half-open `[start_utc_ms, end_utc_ms)` interval with optional witness uncertainty note.
    BoundedInterval {
        start_utc_ms: i64,
        end_utc_ms: i64,
        witness_uncertainty_note: Option<String>,
    },
    /// Calendar date without a known time-of-day. Never promoted to a midnight
    /// instant (`docs/architecture.md` §3.1).
    CalendarDateOnly {
        year: i32,
        month: u8,
        day: u8,
        approximate_day_bounds_utc_ms: (i64, i64),
    },
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: i32, month: u8) -> Option<u8> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 => Some(if is_leap_year(year) { 29 } else { 28 }),
        _ => None,
    }
}

/// Proleptic Gregorian days since `1970-01-01` (Howard Hinnant algorithm).
fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let y = i64::from(year) - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = i64::from(month);
    let d = i64::from(day);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Reported event time preserving original text, timezone, offset ambiguity,
/// calendar-only precision, and earliest-known-source qualification (`GEO-01`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportedEventTime {
    reported_local_text: String,
    timezone_name: String,
    utc_offset_minutes: Option<i16>,
    precision: ReportedTimePrecision,
    earliest_known_source: Option<EarliestKnownSourceQualification>,
}

impl ReportedEventTime {
    pub fn new(
        reported_local_text: impl Into<String>,
        timezone_name: impl Into<String>,
        utc_offset_minutes: Option<i16>,
        precision: ReportedTimePrecision,
        earliest_known_source: Option<EarliestKnownSourceQualification>,
        max_text_bytes: usize,
    ) -> Result<Self, GeoWorkspaceError> {
        let reported_local_text = reported_local_text.into();
        let timezone_name = timezone_name.into();
        validate_text("reported_local_text", &reported_local_text, max_text_bytes)?;
        validate_text("timezone_name", &timezone_name, max_text_bytes)?;
        if let Some(offset) = utc_offset_minutes
            && !(-840..=840).contains(&offset)
        {
            return Err(GeoWorkspaceError::InvalidUtcOffsetMinutes(offset));
        }
        if let ReportedTimePrecision::BoundedInterval {
            start_utc_ms,
            end_utc_ms,
            ref witness_uncertainty_note,
        } = precision
        {
            if start_utc_ms >= end_utc_ms {
                return Err(GeoWorkspaceError::InvalidTimeInterval {
                    start_utc_ms,
                    end_utc_ms,
                });
            }
            if let Some(note) = witness_uncertainty_note {
                validate_text("witness_uncertainty_note", note, max_text_bytes)?;
            }
        }
        Ok(Self {
            reported_local_text,
            timezone_name,
            utc_offset_minutes,
            precision,
            earliest_known_source,
        })
    }

    /// Constructs a calendar-date-only historical record (`docs/architecture.md` §3.1).
    ///
    /// When `utc_offset_minutes` is `None`, the full `[-14h, +14h]` timezone uncertainty
    /// envelope around that calendar date is used for coarse timeline overlap while keeping
    /// `is_calendar_date_only() == true`.
    #[allow(clippy::too_many_arguments)]
    pub fn calendar_date_only(
        reported_local_text: impl Into<String>,
        timezone_name: impl Into<String>,
        utc_offset_minutes: Option<i16>,
        year: i32,
        month: u8,
        day: u8,
        earliest_known_source: Option<EarliestKnownSourceQualification>,
        max_text_bytes: usize,
    ) -> Result<Self, GeoWorkspaceError> {
        let Some(max_day) = days_in_month(year, month) else {
            return Err(GeoWorkspaceError::InvalidCalendarDate { year, month, day });
        };
        if day == 0 || day > max_day || !(-9999..=9999).contains(&year) {
            return Err(GeoWorkspaceError::InvalidCalendarDate { year, month, day });
        }
        let day_start_utc_ms = days_from_civil(year, month, day) * 86_400_000;
        let bounds = match utc_offset_minutes {
            Some(offset) => {
                if !(-840..=840).contains(&offset) {
                    return Err(GeoWorkspaceError::InvalidUtcOffsetMinutes(offset));
                }
                let shift_ms = i64::from(offset) * 60_000;
                let start = day_start_utc_ms - shift_ms;
                (start, start + 86_400_000)
            }
            None => (
                day_start_utc_ms - 14 * 3_600_000,
                day_start_utc_ms + 86_400_000 + 14 * 3_600_000,
            ),
        };
        Self::new(
            reported_local_text,
            timezone_name,
            utc_offset_minutes,
            ReportedTimePrecision::CalendarDateOnly {
                year,
                month,
                day,
                approximate_day_bounds_utc_ms: bounds,
            },
            earliest_known_source,
            max_text_bytes,
        )
    }

    #[must_use]
    pub fn reported_local_text(&self) -> &str {
        &self.reported_local_text
    }

    #[must_use]
    pub fn timezone_name(&self) -> &str {
        &self.timezone_name
    }

    #[must_use]
    pub const fn utc_offset_minutes(&self) -> Option<i16> {
        self.utc_offset_minutes
    }

    #[must_use]
    pub const fn precision(&self) -> &ReportedTimePrecision {
        &self.precision
    }

    #[must_use]
    pub const fn earliest_known_source(&self) -> Option<&EarliestKnownSourceQualification> {
        self.earliest_known_source.as_ref()
    }

    /// Returns `true` when only a calendar date was reported (never a midnight timestamp).
    #[must_use]
    pub const fn is_calendar_date_only(&self) -> bool {
        matches!(
            self.precision,
            ReportedTimePrecision::CalendarDateOnly { .. }
        )
    }

    /// Returns the coarse `[start_utc_ms, end_utc_ms]` envelope for timeline overlap checks.
    #[must_use]
    pub const fn comparison_bounds_utc_ms(&self) -> (i64, i64) {
        match self.precision {
            ReportedTimePrecision::ExactInstant { utc_ms } => (utc_ms, utc_ms),
            ReportedTimePrecision::BoundedInterval {
                start_utc_ms,
                end_utc_ms,
                ..
            } => (start_utc_ms, end_utc_ms),
            ReportedTimePrecision::CalendarDateOnly {
                approximate_day_bounds_utc_ms,
                ..
            } => approximate_day_bounds_utc_ms,
        }
    }

    /// Minimum temporal separation in milliseconds between two reported event times (`0` if overlapping).
    #[must_use]
    pub fn minimum_gap_ms(&self, other: &Self) -> u64 {
        let (a_start, a_end) = self.comparison_bounds_utc_ms();
        let (b_start, b_end) = other.comparison_bounds_utc_ms();
        if a_end < b_start {
            b_start.abs_diff(a_end)
        } else if b_end < a_start {
            a_start.abs_diff(b_end)
        } else {
            0
        }
    }
}

/// Source modality queried for an investigation area/time (`GEO-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GeoModality {
    SatelliteImagery,
    PublicCameraRecords,
    WeatherContext,
    AircraftVesselActivity,
    AstronomicalContext,
    WitnessReports,
    Documents,
}

/// Source availability and coverage status for a queried modality (`GEO-02`, `A04`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalitySourceStatus {
    Available,
    PartialCoverage,
    StaleSnapshot { age_ms: u64 },
    MissingHistory,
    Unavailable,
}

/// Distinguishes direct observations from display interpolation and incomplete tracks (`GEO-05`, `A04`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackDataBasis {
    DirectObservationsOnly,
    ContainsDisplayInterpolation { max_gap_ms: u64 },
    IncompleteTrackWithDropouts { dropout_windows: u32 },
    NotApplicable,
}

/// Per-source coverage record within a [`GeoCoverageEnvelope`] (`GEO-02`).
#[derive(Debug, Clone, PartialEq)]
pub struct ModalityCoverageRecord {
    pub modality: GeoModality,
    pub source_id: OpaqueId,
    pub source_version: VersionId,
    pub status: ModalitySourceStatus,
    pub captured_interval_utc_ms: Option<(i64, i64)>,
    pub spatial_bounds: Option<GeoBoundingBoxWgs84>,
    pub track_basis: TrackDataBasis,
    pub resolution_or_calibration_limit: String,
}

impl ModalityCoverageRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        modality: GeoModality,
        source_id: OpaqueId,
        source_version: VersionId,
        status: ModalitySourceStatus,
        captured_interval_utc_ms: Option<(i64, i64)>,
        spatial_bounds: Option<GeoBoundingBoxWgs84>,
        track_basis: TrackDataBasis,
        resolution_or_calibration_limit: impl Into<String>,
        max_text_bytes: usize,
    ) -> Result<Self, GeoWorkspaceError> {
        if let Some((start_utc_ms, end_utc_ms)) = captured_interval_utc_ms
            && start_utc_ms >= end_utc_ms
        {
            return Err(GeoWorkspaceError::InvalidTimeInterval {
                start_utc_ms,
                end_utc_ms,
            });
        }
        let resolution_or_calibration_limit = resolution_or_calibration_limit.into();
        validate_text(
            "resolution_or_calibration_limit",
            &resolution_or_calibration_limit,
            max_text_bytes,
        )?;
        Ok(Self {
            modality,
            source_id,
            source_version,
            status,
            captured_interval_utc_ms,
            spatial_bounds,
            track_basis,
            resolution_or_calibration_limit,
        })
    }
}

/// Explicit scene coverage conclusion preventing missing feeds from becoming
/// "clear sky" or "empty scene" conclusions (`GEO-02`, `GEO-05`, `A04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneCoverageConclusion {
    /// Every requested modality has complete, fresh observational coverage.
    CompleteObservationalCoverage,
    /// One or more modalities are missing history, stale, partial, or interpolated;
    /// an absent track or missing feed must NOT be treated as an empty scene (`A04`).
    CoverageGapCannotInferEmptyScene {
        missing_or_unavailable_modalities: Vec<GeoModality>,
        stale_or_partial_modalities: Vec<GeoModality>,
        interpolated_or_incomplete_track_sources: Vec<OpaqueId>,
    },
}

/// Multi-modality coverage envelope for a geographic/temporal query (`GEO-02`, `A04`).
#[derive(Debug, Clone, PartialEq)]
pub struct GeoCoverageEnvelope {
    pub query_id: OpaqueId,
    pub requested_bounds: GeoBoundingBoxWgs84,
    pub requested_interval_utc_ms: (i64, i64),
    pub required_modalities: BTreeSet<GeoModality>,
    pub records: Vec<ModalityCoverageRecord>,
}

impl GeoCoverageEnvelope {
    pub fn new(
        query_id: OpaqueId,
        requested_bounds: GeoBoundingBoxWgs84,
        requested_start_utc_ms: i64,
        requested_end_utc_ms: i64,
        required_modalities: BTreeSet<GeoModality>,
        records: Vec<ModalityCoverageRecord>,
    ) -> Result<Self, GeoWorkspaceError> {
        if requested_start_utc_ms >= requested_end_utc_ms {
            return Err(GeoWorkspaceError::InvalidTimeInterval {
                start_utc_ms: requested_start_utc_ms,
                end_utc_ms: requested_end_utc_ms,
            });
        }
        Ok(Self {
            query_id,
            requested_bounds,
            requested_interval_utc_ms: (requested_start_utc_ms, requested_end_utc_ms),
            required_modalities,
            records,
        })
    }

    /// Evaluates whether the queried modalities support complete observational coverage
    /// or contain gaps that forbid an empty-scene conclusion (`GEO-02`, `GEO-05`, `A04`).
    #[must_use]
    pub fn evaluate_scene_conclusion(&self) -> SceneCoverageConclusion {
        let mut missing_or_unavailable = BTreeSet::new();
        let mut stale_or_partial = BTreeSet::new();
        let mut interpolated_or_incomplete = Vec::new();

        for modality in &self.required_modalities {
            let matching: Vec<&ModalityCoverageRecord> = self
                .records
                .iter()
                .filter(|r| r.modality == *modality)
                .collect();
            if matching.is_empty() {
                missing_or_unavailable.insert(*modality);
                continue;
            }
            for rec in matching {
                match rec.status {
                    ModalitySourceStatus::Available => {
                        let time_covers = rec.captured_interval_utc_ms.is_some_and(|(s, e)| {
                            s <= self.requested_interval_utc_ms.0
                                && e >= self.requested_interval_utc_ms.1
                        });
                        let space_covers = rec
                            .spatial_bounds
                            .is_some_and(|b| b.intersects_box(self.requested_bounds));
                        if !time_covers || !space_covers {
                            stale_or_partial.insert(*modality);
                        }
                    }
                    ModalitySourceStatus::PartialCoverage
                    | ModalitySourceStatus::StaleSnapshot { .. } => {
                        stale_or_partial.insert(*modality);
                    }
                    ModalitySourceStatus::MissingHistory | ModalitySourceStatus::Unavailable => {
                        missing_or_unavailable.insert(*modality);
                    }
                }
                if matches!(
                    rec.track_basis,
                    TrackDataBasis::ContainsDisplayInterpolation { .. }
                        | TrackDataBasis::IncompleteTrackWithDropouts { .. }
                ) {
                    interpolated_or_incomplete.push(rec.source_id.clone());
                }
            }
        }

        if missing_or_unavailable.is_empty()
            && stale_or_partial.is_empty()
            && interpolated_or_incomplete.is_empty()
        {
            SceneCoverageConclusion::CompleteObservationalCoverage
        } else {
            SceneCoverageConclusion::CoverageGapCannotInferEmptyScene {
                missing_or_unavailable_modalities: missing_or_unavailable.into_iter().collect(),
                stale_or_partial_modalities: stale_or_partial.into_iter().collect(),
                interpolated_or_incomplete_track_sources: interpolated_or_incomplete,
            }
        }
    }
}

/// Classification of an evidence item participating in geographic clustering (`GEO-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeoEvidenceKind {
    /// Secondary or news article (counted in `article_count`).
    PublishedArticle,
    /// Primary witness account; `AccountOrigin::Simulated` items are excluded from
    /// observed `account_count` and `independent_source_count` (`AGENTS.md` §2).
    WitnessAccount { origin: AccountOrigin },
    /// Sensor, radar, or optical observation; `AccountOrigin::Simulated` items are
    /// excluded from observed `independent_source_count`.
    SensorObservation { origin: AccountOrigin },
}

impl GeoEvidenceKind {
    #[must_use]
    pub const fn is_simulated(self) -> bool {
        matches!(
            self,
            Self::WitnessAccount {
                origin: AccountOrigin::Simulated
            } | Self::SensorObservation {
                origin: AccountOrigin::Simulated
            }
        )
    }
}

/// Lineage relationship between evidence items for independent-source calculation (`GEO-03`, `AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceLineageRelation {
    IndependentRoot,
    Republishes { upstream_item_id: OpaqueId },
    Quotes { upstream_item_id: OpaqueId },
    Copies { upstream_item_id: OpaqueId },
}

impl SourceLineageRelation {
    #[must_use]
    pub const fn upstream_item_id(&self) -> Option<&OpaqueId> {
        match self {
            Self::IndependentRoot => None,
            Self::Republishes { upstream_item_id }
            | Self::Quotes { upstream_item_id }
            | Self::Copies { upstream_item_id } => Some(upstream_item_id),
        }
    }
}

/// Evidence record registered for geographic event clustering (`GEO-01`, `GEO-03`).
#[derive(Debug, Clone, PartialEq)]
pub struct GeoEvidenceItem {
    pub item_id: OpaqueId,
    pub version_id: VersionId,
    pub source_id: OpaqueId,
    pub content_hash: Option<String>,
    pub kind: GeoEvidenceKind,
    pub lineage: SourceLineageRelation,
    pub reported_time: ReportedEventTime,
    pub location: GeoLocation,
    pub reported_properties: BTreeMap<String, String>,
}

impl GeoEvidenceItem {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        item_id: OpaqueId,
        version_id: VersionId,
        source_id: OpaqueId,
        content_hash: Option<String>,
        kind: GeoEvidenceKind,
        lineage: SourceLineageRelation,
        reported_time: ReportedEventTime,
        location: GeoLocation,
        reported_properties: BTreeMap<String, String>,
        max_text_bytes: usize,
    ) -> Result<Self, GeoWorkspaceError> {
        if lineage.upstream_item_id() == Some(&item_id) {
            return Err(GeoWorkspaceError::SelfLineageReference(item_id));
        }
        if let Some(ref hash) = content_hash {
            validate_text("content_hash", hash, max_text_bytes)?;
        }
        for (k, v) in &reported_properties {
            validate_text("property_key", k, max_text_bytes)?;
            validate_text("property_value", v, max_text_bytes)?;
        }
        Ok(Self {
            item_id,
            version_id,
            source_id,
            content_hash,
            kind,
            lineage,
            reported_time,
            location,
            reported_properties,
        })
    }
}

/// Matching criteria recorded on an event cluster (`GEO-03`).
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterMatchCriteria {
    pub max_proximity_m: f64,
    pub max_time_separation_ms: u64,
    pub required_property_keys: BTreeSet<String>,
}

impl ClusterMatchCriteria {
    pub fn new(
        max_proximity_m: f64,
        max_time_separation_ms: u64,
        required_property_keys: BTreeSet<String>,
    ) -> Result<Self, GeoWorkspaceError> {
        let max_proximity_m = validate_positive_distance("max_proximity_m", max_proximity_m)?;
        Ok(Self {
            max_proximity_m,
            max_time_separation_ms,
            required_property_keys,
        })
    }
}

/// Conflict or qualification surfaced within an event cluster (`GEO-03`, `A04`).
#[derive(Debug, Clone, PartialEq)]
pub enum ClusterConflict {
    /// Two items in the cluster exceed `criteria.max_proximity_m`.
    SpatialSeparationExceedsThreshold {
        item_a: OpaqueId,
        item_b: OpaqueId,
        distance_m: f64,
        threshold_m: f64,
    },
    /// Two items in the cluster exceed `criteria.max_time_separation_ms`.
    TemporalSeparationExceedsThreshold {
        item_a: OpaqueId,
        item_b: OpaqueId,
        gap_ms: u64,
        threshold_ms: u64,
    },
    /// Members report conflicting values for the same property key (e.g., `"shape"` or `"direction"`).
    PropertyMismatch {
        property_key: String,
        values_by_item: BTreeMap<OpaqueId, String>,
    },
    /// Member uses an approximate gazetteer centroid or bounded area rather than an exact point.
    ApproximateLocationUsed {
        item_id: OpaqueId,
        radius_uncertainty_m: Option<f64>,
    },
    /// Member has only a calendar date or earliest-known-source qualification.
    HistoricalTimeQualificationPresent {
        item_id: OpaqueId,
        calendar_date_only: bool,
        earliest_known_source_id: Option<OpaqueId>,
    },
    /// Member is a controlled simulation (`AccountOrigin::Simulated`) and is excluded
    /// from observed account and independent-source counts.
    SimulatedEvidencePresent { item_id: OpaqueId },
    /// Lineage edges form an unanchored citation cycle with no independent root.
    UnanchoredLineageCycle { cycle_members: Vec<OpaqueId> },
}

/// Strictly separate counts of articles, witness accounts, independent sources,
/// proposed events, and simulated items (`GEO-03`, `A04`, `AGENTS.md` §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistinctEvidenceCounts {
    pub article_count: usize,
    pub account_count: usize,
    pub independent_source_count: usize,
    pub proposed_event_count: usize,
    pub simulated_item_count: usize,
}

/// Immutable snapshot of a proposed event cluster (`GEO-03`).
#[derive(Debug, Clone, PartialEq)]
pub struct ProposedEventCluster {
    pub cluster_id: OpaqueId,
    pub version_id: VersionId,
    pub member_item_ids: BTreeSet<OpaqueId>,
    pub criteria: ClusterMatchCriteria,
    pub conflicts: Vec<ClusterConflict>,
    pub counts: DistinctEvidenceCounts,
    pub rationale: String,
}

/// Reversible cluster mutation kind (`GEO-03`).
#[derive(Debug, Clone, PartialEq)]
pub enum ClusterDecisionKind {
    Merge {
        source_cluster_a: ProposedEventCluster,
        source_cluster_b: ProposedEventCluster,
        merged_cluster_id: OpaqueId,
        merged_version_id: VersionId,
    },
    Split {
        original_cluster: ProposedEventCluster,
        left_cluster_id: OpaqueId,
        left_version_id: VersionId,
        right_cluster_id: OpaqueId,
        right_version_id: VersionId,
    },
    Revert {
        reverted_decision_id: OpaqueId,
        restored_cluster_versions: Vec<(OpaqueId, VersionId)>,
    },
}

/// Audit record for an editable clustering decision (`GEO-03`).
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterDecisionRecord {
    pub decision_id: OpaqueId,
    pub actor: ActorId,
    pub kind: ClusterDecisionKind,
    pub rationale: String,
    pub reverted_by: Option<OpaqueId>,
}

/// Source-aware, editable, and reversible event clustering ledger (`GEO-03`, `A04`).
#[derive(Debug, Clone, PartialEq)]
pub struct EventClusterLedger {
    schema_version: SchemaVersion,
    limits: GeoWorkspaceLimits,
    items: BTreeMap<OpaqueId, GeoEvidenceItem>,
    clusters: BTreeMap<OpaqueId, ProposedEventCluster>,
    used_versions: BTreeSet<VersionId>,
    decision_history: Vec<ClusterDecisionRecord>,
}

impl EventClusterLedger {
    #[must_use]
    pub fn new(limits: GeoWorkspaceLimits) -> Self {
        Self {
            schema_version: SchemaVersion::V1,
            limits,
            items: BTreeMap::new(),
            clusters: BTreeMap::new(),
            used_versions: BTreeSet::new(),
            decision_history: Vec::new(),
        }
    }

    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    #[must_use]
    pub const fn items(&self) -> &BTreeMap<OpaqueId, GeoEvidenceItem> {
        &self.items
    }

    #[must_use]
    pub const fn clusters(&self) -> &BTreeMap<OpaqueId, ProposedEventCluster> {
        &self.clusters
    }

    #[must_use]
    pub fn decision_history(&self) -> &[ClusterDecisionRecord] {
        &self.decision_history
    }

    /// Registers an evidence item and places it in its own initial cluster.
    pub fn register_item_with_initial_cluster(
        &mut self,
        item: GeoEvidenceItem,
        initial_cluster_id: OpaqueId,
        initial_cluster_version: VersionId,
        criteria: ClusterMatchCriteria,
        rationale: impl Into<String>,
    ) -> Result<(), GeoWorkspaceError> {
        if self.items.len() >= self.limits.max_evidence_items {
            return Err(GeoWorkspaceError::ResourceLimitExceeded(
                "max_evidence_items",
            ));
        }
        if self.clusters.len() >= self.limits.max_clusters {
            return Err(GeoWorkspaceError::ResourceLimitExceeded("max_clusters"));
        }
        if self.items.contains_key(&item.item_id) {
            return Err(GeoWorkspaceError::DuplicateEvidenceItem(item.item_id));
        }
        if self.clusters.contains_key(&initial_cluster_id) {
            return Err(GeoWorkspaceError::DuplicateClusterId(initial_cluster_id));
        }
        if self.used_versions.contains(&item.version_id) {
            return Err(GeoWorkspaceError::DuplicateVersionId(item.version_id));
        }
        if self.used_versions.contains(&initial_cluster_version)
            || item.version_id == initial_cluster_version
        {
            return Err(GeoWorkspaceError::DuplicateVersionId(
                initial_cluster_version,
            ));
        }
        let rationale = rationale.into();
        validate_text("rationale", &rationale, self.limits.max_text_bytes)?;

        let item_id = item.item_id.clone();
        self.used_versions.insert(item.version_id.clone());
        self.used_versions.insert(initial_cluster_version.clone());
        self.items.insert(item_id.clone(), item);

        let mut members = BTreeSet::new();
        members.insert(item_id);
        let cluster = self.build_cluster_snapshot(
            initial_cluster_id.clone(),
            initial_cluster_version,
            members,
            criteria,
            rationale,
        )?;
        self.clusters.insert(initial_cluster_id, cluster);
        Ok(())
    }

    /// Merges two existing clusters into a new cluster version while recording a
    /// reversible decision entry (`GEO-03`, `A04`).
    #[allow(clippy::too_many_arguments)]
    pub fn merge_clusters(
        &mut self,
        cluster_a_id: &OpaqueId,
        cluster_b_id: &OpaqueId,
        merged_cluster_id: OpaqueId,
        merged_version_id: VersionId,
        criteria: ClusterMatchCriteria,
        decision_id: OpaqueId,
        actor: ActorId,
        rationale: impl Into<String>,
    ) -> Result<&ProposedEventCluster, GeoWorkspaceError> {
        if cluster_a_id == cluster_b_id {
            return Err(GeoWorkspaceError::DuplicateClusterId(cluster_a_id.clone()));
        }
        if self.decision_history.len() >= self.limits.max_decision_history {
            return Err(GeoWorkspaceError::ResourceLimitExceeded(
                "max_decision_history",
            ));
        }
        if self
            .decision_history
            .iter()
            .any(|d| d.decision_id == decision_id)
        {
            return Err(GeoWorkspaceError::DuplicateDecisionId(decision_id));
        }
        if self.used_versions.contains(&merged_version_id) {
            return Err(GeoWorkspaceError::DuplicateVersionId(merged_version_id));
        }
        if merged_cluster_id != *cluster_a_id
            && merged_cluster_id != *cluster_b_id
            && self.clusters.contains_key(&merged_cluster_id)
        {
            return Err(GeoWorkspaceError::DuplicateClusterId(merged_cluster_id));
        }
        let rationale = rationale.into();
        validate_text("rationale", &rationale, self.limits.max_text_bytes)?;

        let Some(cluster_a) = self.clusters.get(cluster_a_id).cloned() else {
            return Err(GeoWorkspaceError::UnknownClusterId(cluster_a_id.clone()));
        };
        let Some(cluster_b) = self.clusters.get(cluster_b_id).cloned() else {
            return Err(GeoWorkspaceError::UnknownClusterId(cluster_b_id.clone()));
        };

        let mut union_members = cluster_a.member_item_ids.clone();
        union_members.extend(cluster_b.member_item_ids.iter().cloned());

        let merged_snapshot = self.build_cluster_snapshot(
            merged_cluster_id.clone(),
            merged_version_id.clone(),
            union_members,
            criteria,
            rationale.clone(),
        )?;

        self.clusters.remove(cluster_a_id);
        self.clusters.remove(cluster_b_id);
        self.used_versions.insert(merged_version_id.clone());
        self.clusters
            .insert(merged_cluster_id.clone(), merged_snapshot);
        self.decision_history.push(ClusterDecisionRecord {
            decision_id,
            actor,
            kind: ClusterDecisionKind::Merge {
                source_cluster_a: cluster_a,
                source_cluster_b: cluster_b,
                merged_cluster_id: merged_cluster_id.clone(),
                merged_version_id,
            },
            rationale,
            reverted_by: None,
        });
        self.clusters
            .get(&merged_cluster_id)
            .ok_or(GeoWorkspaceError::UnknownClusterId(merged_cluster_id))
    }

    /// Splits a cluster into two disjoint, non-empty child clusters while recording a
    /// reversible decision entry (`GEO-03`, `A04`).
    #[allow(clippy::too_many_arguments)]
    pub fn split_cluster(
        &mut self,
        source_cluster_id: &OpaqueId,
        left_cluster_id: OpaqueId,
        left_version_id: VersionId,
        left_members: BTreeSet<OpaqueId>,
        right_cluster_id: OpaqueId,
        right_version_id: VersionId,
        right_members: BTreeSet<OpaqueId>,
        criteria: ClusterMatchCriteria,
        decision_id: OpaqueId,
        actor: ActorId,
        rationale: impl Into<String>,
    ) -> Result<(), GeoWorkspaceError> {
        if self.clusters.len() >= self.limits.max_clusters {
            return Err(GeoWorkspaceError::ResourceLimitExceeded("max_clusters"));
        }
        if self.decision_history.len() >= self.limits.max_decision_history {
            return Err(GeoWorkspaceError::ResourceLimitExceeded(
                "max_decision_history",
            ));
        }
        if left_cluster_id == right_cluster_id {
            return Err(GeoWorkspaceError::DuplicateClusterId(left_cluster_id));
        }
        if left_version_id == right_version_id {
            return Err(GeoWorkspaceError::DuplicateVersionId(left_version_id));
        }
        if self.used_versions.contains(&left_version_id) {
            return Err(GeoWorkspaceError::DuplicateVersionId(left_version_id));
        }
        if self.used_versions.contains(&right_version_id) {
            return Err(GeoWorkspaceError::DuplicateVersionId(right_version_id));
        }
        if self
            .decision_history
            .iter()
            .any(|d| d.decision_id == decision_id)
        {
            return Err(GeoWorkspaceError::DuplicateDecisionId(decision_id));
        }
        let rationale = rationale.into();
        validate_text("rationale", &rationale, self.limits.max_text_bytes)?;

        let Some(original) = self.clusters.get(source_cluster_id).cloned() else {
            return Err(GeoWorkspaceError::UnknownClusterId(
                source_cluster_id.clone(),
            ));
        };
        if left_cluster_id != *source_cluster_id && self.clusters.contains_key(&left_cluster_id) {
            return Err(GeoWorkspaceError::DuplicateClusterId(left_cluster_id));
        }
        if right_cluster_id != *source_cluster_id && self.clusters.contains_key(&right_cluster_id) {
            return Err(GeoWorkspaceError::DuplicateClusterId(right_cluster_id));
        }
        if left_members.is_empty() || right_members.is_empty() {
            return Err(GeoWorkspaceError::InvalidClusterSplitPartition {
                cluster_id: source_cluster_id.clone(),
                reason: "both split partitions must be non-empty",
            });
        }
        if !left_members.is_disjoint(&right_members) {
            return Err(GeoWorkspaceError::InvalidClusterSplitPartition {
                cluster_id: source_cluster_id.clone(),
                reason: "split partitions must be disjoint",
            });
        }
        let mut combined = left_members.clone();
        combined.extend(right_members.iter().cloned());
        if combined != original.member_item_ids {
            return Err(GeoWorkspaceError::InvalidClusterSplitPartition {
                cluster_id: source_cluster_id.clone(),
                reason: "split partitions must equal the original cluster members",
            });
        }

        let left_snapshot = self.build_cluster_snapshot(
            left_cluster_id.clone(),
            left_version_id.clone(),
            left_members,
            criteria.clone(),
            rationale.clone(),
        )?;
        let right_snapshot = self.build_cluster_snapshot(
            right_cluster_id.clone(),
            right_version_id.clone(),
            right_members,
            criteria,
            rationale.clone(),
        )?;

        self.clusters.remove(source_cluster_id);
        self.used_versions.insert(left_version_id.clone());
        self.used_versions.insert(right_version_id.clone());
        self.clusters.insert(left_cluster_id.clone(), left_snapshot);
        self.clusters
            .insert(right_cluster_id.clone(), right_snapshot);
        self.decision_history.push(ClusterDecisionRecord {
            decision_id,
            actor,
            kind: ClusterDecisionKind::Split {
                original_cluster: original,
                left_cluster_id,
                left_version_id,
                right_cluster_id,
                right_version_id,
            },
            rationale,
            reverted_by: None,
        });
        Ok(())
    }

    /// Reverts a prior merge or split decision by restoring the pre-decision clusters
    /// under new immutable [`VersionId`]s while preserving the decision audit history (`GEO-03`, `A04`).
    pub fn revert_decision(
        &mut self,
        target_decision_id: &OpaqueId,
        new_revert_decision_id: OpaqueId,
        restored_version_ids: &[VersionId],
        actor: ActorId,
        rationale: impl Into<String>,
    ) -> Result<(), GeoWorkspaceError> {
        if self.decision_history.len() >= self.limits.max_decision_history {
            return Err(GeoWorkspaceError::ResourceLimitExceeded(
                "max_decision_history",
            ));
        }
        if self
            .decision_history
            .iter()
            .any(|d| d.decision_id == new_revert_decision_id)
        {
            return Err(GeoWorkspaceError::DuplicateDecisionId(
                new_revert_decision_id,
            ));
        }
        let rationale = rationale.into();
        validate_text("rationale", &rationale, self.limits.max_text_bytes)?;

        let mut unique_new_versions = BTreeSet::new();
        for v in restored_version_ids {
            if self.used_versions.contains(v) || !unique_new_versions.insert(v.clone()) {
                return Err(GeoWorkspaceError::DuplicateVersionId(v.clone()));
            }
        }

        let Some(target_idx) = self
            .decision_history
            .iter()
            .position(|d| &d.decision_id == target_decision_id)
        else {
            return Err(GeoWorkspaceError::UnknownDecisionId(
                target_decision_id.clone(),
            ));
        };
        if self.decision_history[target_idx].reverted_by.is_some() {
            return Err(GeoWorkspaceError::DecisionAlreadyReverted(
                target_decision_id.clone(),
            ));
        }

        let target_kind = self.decision_history[target_idx].kind.clone();
        let mut restored_pairs = Vec::new();

        match target_kind {
            ClusterDecisionKind::Merge {
                source_cluster_a,
                source_cluster_b,
                merged_cluster_id,
                merged_version_id,
            } => {
                let [ver_a, ver_b] = restored_version_ids else {
                    return Err(GeoWorkspaceError::InvalidClusterSplitPartition {
                        cluster_id: merged_cluster_id,
                        reason: "reverting a merge requires exactly 2 new VersionIds",
                    });
                };
                let current = self.clusters.get(&merged_cluster_id);
                if current.map(|c| &c.version_id) != Some(&merged_version_id) {
                    return Err(GeoWorkspaceError::DecisionNoLongerCurrent {
                        decision_id: target_decision_id.clone(),
                        missing_or_modified_cluster: merged_cluster_id,
                    });
                }
                let restored_a = self.build_cluster_snapshot(
                    source_cluster_a.cluster_id.clone(),
                    ver_a.clone(),
                    source_cluster_a.member_item_ids,
                    source_cluster_a.criteria,
                    rationale.clone(),
                )?;
                let restored_b = self.build_cluster_snapshot(
                    source_cluster_b.cluster_id.clone(),
                    ver_b.clone(),
                    source_cluster_b.member_item_ids,
                    source_cluster_b.criteria,
                    rationale.clone(),
                )?;
                self.clusters.remove(&merged_cluster_id);
                self.used_versions.insert(ver_a.clone());
                self.used_versions.insert(ver_b.clone());
                restored_pairs.push((restored_a.cluster_id.clone(), ver_a.clone()));
                restored_pairs.push((restored_b.cluster_id.clone(), ver_b.clone()));
                self.clusters
                    .insert(restored_a.cluster_id.clone(), restored_a);
                self.clusters
                    .insert(restored_b.cluster_id.clone(), restored_b);
            }
            ClusterDecisionKind::Split {
                original_cluster,
                left_cluster_id,
                left_version_id,
                right_cluster_id,
                right_version_id,
            } => {
                let [ver_orig] = restored_version_ids else {
                    return Err(GeoWorkspaceError::InvalidClusterSplitPartition {
                        cluster_id: original_cluster.cluster_id,
                        reason: "reverting a split requires exactly 1 new VersionId",
                    });
                };
                if self.clusters.get(&left_cluster_id).map(|c| &c.version_id)
                    != Some(&left_version_id)
                {
                    return Err(GeoWorkspaceError::DecisionNoLongerCurrent {
                        decision_id: target_decision_id.clone(),
                        missing_or_modified_cluster: left_cluster_id,
                    });
                }
                if self.clusters.get(&right_cluster_id).map(|c| &c.version_id)
                    != Some(&right_version_id)
                {
                    return Err(GeoWorkspaceError::DecisionNoLongerCurrent {
                        decision_id: target_decision_id.clone(),
                        missing_or_modified_cluster: right_cluster_id,
                    });
                }
                let restored_orig = self.build_cluster_snapshot(
                    original_cluster.cluster_id.clone(),
                    ver_orig.clone(),
                    original_cluster.member_item_ids,
                    original_cluster.criteria,
                    rationale.clone(),
                )?;
                self.clusters.remove(&left_cluster_id);
                self.clusters.remove(&right_cluster_id);
                self.used_versions.insert(ver_orig.clone());
                restored_pairs.push((restored_orig.cluster_id.clone(), ver_orig.clone()));
                self.clusters
                    .insert(restored_orig.cluster_id.clone(), restored_orig);
            }
            ClusterDecisionKind::Revert { .. } => {
                return Err(GeoWorkspaceError::DecisionAlreadyReverted(
                    target_decision_id.clone(),
                ));
            }
        }

        self.decision_history[target_idx].reverted_by = Some(new_revert_decision_id.clone());
        self.decision_history.push(ClusterDecisionRecord {
            decision_id: new_revert_decision_id,
            actor,
            kind: ClusterDecisionKind::Revert {
                reverted_decision_id: target_decision_id.clone(),
                restored_cluster_versions: restored_pairs,
            },
            rationale,
            reverted_by: None,
        });
        Ok(())
    }

    /// Computes ledger-wide distinct counts of articles, witness accounts,
    /// independent sources, proposed event clusters, and simulated items (`GEO-03`, `A04`).
    #[must_use]
    pub fn ledger_wide_counts(&self) -> DistinctEvidenceCounts {
        let all_ids: BTreeSet<OpaqueId> = self.items.keys().cloned().collect();
        let (mut counts, _) = self.compute_counts_and_cycles(&all_ids);
        counts.proposed_event_count = self.clusters.len();
        counts
    }

    fn build_cluster_snapshot(
        &self,
        cluster_id: OpaqueId,
        version_id: VersionId,
        member_item_ids: BTreeSet<OpaqueId>,
        criteria: ClusterMatchCriteria,
        rationale: String,
    ) -> Result<ProposedEventCluster, GeoWorkspaceError> {
        let mut members = Vec::with_capacity(member_item_ids.len());
        for id in &member_item_ids {
            let Some(item) = self.items.get(id) else {
                return Err(GeoWorkspaceError::UnknownEvidenceItem(id.clone()));
            };
            members.push(item);
        }

        let mut conflicts = Vec::new();
        for (idx_a, item_a) in members.iter().enumerate() {
            if let GeoLocation::ApproximateGazetteerCentroid {
                radius_uncertainty_m,
                ..
            } = item_a.location
            {
                conflicts.push(ClusterConflict::ApproximateLocationUsed {
                    item_id: item_a.item_id.clone(),
                    radius_uncertainty_m: Some(radius_uncertainty_m),
                });
            } else if matches!(item_a.location, GeoLocation::BoundedArea { .. }) {
                conflicts.push(ClusterConflict::ApproximateLocationUsed {
                    item_id: item_a.item_id.clone(),
                    radius_uncertainty_m: None,
                });
            }
            if item_a.reported_time.is_calendar_date_only()
                || item_a.reported_time.earliest_known_source().is_some()
            {
                conflicts.push(ClusterConflict::HistoricalTimeQualificationPresent {
                    item_id: item_a.item_id.clone(),
                    calendar_date_only: item_a.reported_time.is_calendar_date_only(),
                    earliest_known_source_id: item_a
                        .reported_time
                        .earliest_known_source()
                        .map(|q| q.earliest_source_id.clone()),
                });
            }
            if item_a.kind.is_simulated() {
                conflicts.push(ClusterConflict::SimulatedEvidencePresent {
                    item_id: item_a.item_id.clone(),
                });
            }

            for item_b in members.iter().skip(idx_a + 1) {
                let dist_m = item_a
                    .location
                    .representative_point()
                    .haversine_distance_m(item_b.location.representative_point());
                if dist_m > criteria.max_proximity_m {
                    conflicts.push(ClusterConflict::SpatialSeparationExceedsThreshold {
                        item_a: item_a.item_id.clone(),
                        item_b: item_b.item_id.clone(),
                        distance_m: dist_m,
                        threshold_m: criteria.max_proximity_m,
                    });
                }
                let gap_ms = item_a.reported_time.minimum_gap_ms(&item_b.reported_time);
                if gap_ms > criteria.max_time_separation_ms {
                    conflicts.push(ClusterConflict::TemporalSeparationExceedsThreshold {
                        item_a: item_a.item_id.clone(),
                        item_b: item_b.item_id.clone(),
                        gap_ms,
                        threshold_ms: criteria.max_time_separation_ms,
                    });
                }
            }
        }

        let mut all_keys: BTreeSet<String> = criteria.required_property_keys.clone();
        for item in &members {
            all_keys.extend(item.reported_properties.keys().cloned());
        }
        for key in all_keys {
            let mut values_by_item = BTreeMap::new();
            let mut distinct_values = BTreeSet::new();
            for item in &members {
                if let Some(val) = item.reported_properties.get(&key) {
                    values_by_item.insert(item.item_id.clone(), val.clone());
                    distinct_values.insert(val.as_str());
                }
            }
            if distinct_values.len() > 1 {
                conflicts.push(ClusterConflict::PropertyMismatch {
                    property_key: key,
                    values_by_item,
                });
            }
        }

        let (counts, cycle_members) = self.compute_counts_and_cycles(&member_item_ids);
        if !cycle_members.is_empty() {
            conflicts.push(ClusterConflict::UnanchoredLineageCycle { cycle_members });
        }

        Ok(ProposedEventCluster {
            cluster_id,
            version_id,
            member_item_ids,
            criteria,
            conflicts,
            counts,
            rationale,
        })
    }

    fn compute_counts_and_cycles(
        &self,
        member_ids: &BTreeSet<OpaqueId>,
    ) -> (DistinctEvidenceCounts, Vec<OpaqueId>) {
        let mut article_count = 0usize;
        let mut account_count = 0usize;
        let mut simulated_item_count = 0usize;

        let mut non_simulated_ids = Vec::new();
        for id in member_ids {
            let Some(item) = self.items.get(id) else {
                continue;
            };
            match item.kind {
                GeoEvidenceKind::PublishedArticle => {
                    article_count += 1;
                    non_simulated_ids.push(id.clone());
                }
                GeoEvidenceKind::WitnessAccount {
                    origin: AccountOrigin::Reported,
                } => {
                    account_count += 1;
                    non_simulated_ids.push(id.clone());
                }
                GeoEvidenceKind::SensorObservation {
                    origin: AccountOrigin::Reported,
                } => {
                    non_simulated_ids.push(id.clone());
                }
                GeoEvidenceKind::WitnessAccount {
                    origin: AccountOrigin::Simulated,
                }
                | GeoEvidenceKind::SensorObservation {
                    origin: AccountOrigin::Simulated,
                } => {
                    simulated_item_count += 1;
                }
            }
        }

        // Collapse lineage chains (`Republishes`/`Quotes`/`Copies`), shared `source_id`,
        // and identical `content_hash` values among non-simulated items (`AGENTS.md` §2).
        let mut cycle_members = BTreeSet::new();
        let mut anchored_items = Vec::new();
        let mut root_item_for = BTreeMap::new();

        for id in &non_simulated_ids {
            let mut current = id.clone();
            let mut visited = BTreeSet::new();
            let mut is_cycle = false;
            while let Some(item) = self.items.get(&current) {
                if item.kind.is_simulated() {
                    is_cycle = true;
                    break;
                }
                if !visited.insert(current.clone()) {
                    is_cycle = true;
                    break;
                }
                match item.lineage.upstream_item_id() {
                    Some(up) => {
                        if self.items.contains_key(up) {
                            current = up.clone();
                        } else {
                            // Upstream item is external to the registered set; anchor at `current`.
                            break;
                        }
                    }
                    None => break,
                }
            }
            if is_cycle {
                cycle_members.insert(id.clone());
            } else {
                anchored_items.push(id.clone());
                root_item_for.insert(id.clone(), current);
            }
        }

        // Union-find across anchored items sharing:
        // 1) the same resolved lineage root item,
        // 2) the same root `source_id`, or
        // 3) the same non-empty `content_hash`.
        let n = anchored_items.len();
        let mut parent: Vec<usize> = (0..n).collect();
        let find = |parent: &mut [usize], mut x: usize| -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        };

        for i in 0..n {
            for j in (i + 1)..n {
                let id_i = &anchored_items[i];
                let id_j = &anchored_items[j];
                let root_i = &root_item_for[id_i];
                let root_j = &root_item_for[id_j];
                let item_i = &self.items[id_i];
                let item_j = &self.items[id_j];
                let root_source_i = self
                    .items
                    .get(root_i)
                    .map(|r| &r.source_id)
                    .unwrap_or(&item_i.source_id);
                let root_source_j = self
                    .items
                    .get(root_j)
                    .map(|r| &r.source_id)
                    .unwrap_or(&item_j.source_id);

                let same_lineage_root = root_i == root_j;
                let same_source =
                    item_i.source_id == item_j.source_id || root_source_i == root_source_j;
                let same_hash = match (&item_i.content_hash, &item_j.content_hash) {
                    (Some(h1), Some(h2)) => h1 == h2,
                    _ => false,
                };

                if same_lineage_root || same_source || same_hash {
                    let ri = find(&mut parent, i);
                    let rj = find(&mut parent, j);
                    if ri != rj {
                        parent[rj] = ri;
                    }
                }
            }
        }

        let mut distinct_roots = BTreeSet::new();
        for i in 0..n {
            distinct_roots.insert(find(&mut parent, i));
        }

        (
            DistinctEvidenceCounts {
                article_count,
                account_count,
                independent_source_count: distinct_roots.len(),
                proposed_event_count: 1,
                simulated_item_count,
            },
            cycle_members.into_iter().collect(),
        )
    }
}

/// Clock synchronization status for a camera cone or imagery footprint (`GEO-04`, `docs/architecture.md` §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClockAlignmentStatus {
    /// Synchronized against a reference clock with explicit offset and uncertainty.
    Synchronized {
        reference_clock: String,
        offset_ms: i64,
        uncertainty_ms: u64,
    },
    /// Clock synchronization was lost; frames must NOT be aligned by receipt time
    /// as if capture times were known (`docs/architecture.md` §3.1).
    LostSyncReceiptTimeOnly { warning: String },
}

/// Camera or sensor calibration state for a mosaic tile (`GEO-04`, `A13`).
#[derive(Debug, Clone, PartialEq)]
pub enum TileCalibrationState {
    VerifiedAtCapture {
        calibration_version: VersionId,
        angular_uncertainty_deg: f64,
    },
    InvalidatedByCameraMovement {
        prior_calibration_version: VersionId,
        movement_reason: String,
    },
    UncalibratedApproximate {
        nominal_fov_deg: f64,
    },
}

/// 2D horizontal viewing cone for a public camera or optical sensor (`GEO-04`).
#[derive(Debug, Clone, PartialEq)]
pub struct ViewingCone2D {
    pub origin: GeoPointWgs84,
    pub center_azimuth_deg: f64,
    pub horizontal_fov_deg: f64,
    pub near_range_m: f64,
    pub far_range_m: f64,
}

impl ViewingCone2D {
    pub fn new(
        origin: GeoPointWgs84,
        center_azimuth_deg: f64,
        horizontal_fov_deg: f64,
        near_range_m: f64,
        far_range_m: f64,
    ) -> Result<Self, GeoWorkspaceError> {
        if !center_azimuth_deg.is_finite() || !(0.0..360.0).contains(&center_azimuth_deg) {
            return Err(GeoWorkspaceError::InvalidAzimuthDeg(center_azimuth_deg));
        }
        if !horizontal_fov_deg.is_finite()
            || horizontal_fov_deg <= 0.0
            || horizontal_fov_deg > 360.0
        {
            return Err(GeoWorkspaceError::InvalidFieldOfViewDeg(horizontal_fov_deg));
        }
        if !near_range_m.is_finite()
            || !far_range_m.is_finite()
            || near_range_m < 0.0
            || near_range_m >= far_range_m
            || far_range_m > MAX_DISTANCE_M
        {
            return Err(GeoWorkspaceError::InvalidRangeBounds {
                near_range_m,
                far_range_m,
            });
        }
        Ok(Self {
            origin,
            center_azimuth_deg,
            horizontal_fov_deg,
            near_range_m,
            far_range_m,
        })
    }

    /// Returns `true` if `target` falls within the cone's range and horizontal azimuth sector.
    #[must_use]
    pub fn contains_point(&self, target: GeoPointWgs84) -> bool {
        let dist_m = self.origin.haversine_distance_m(target);
        if dist_m < self.near_range_m || dist_m > self.far_range_m {
            return false;
        }
        if dist_m == 0.0 || self.horizontal_fov_deg >= 360.0 {
            return true;
        }
        let bearing = self.origin.initial_bearing_deg(target);
        let diff = (bearing - self.center_azimuth_deg + 540.0).rem_euclid(360.0) - 180.0;
        diff.abs() <= 0.5 * self.horizontal_fov_deg
    }
}

/// Geometry of a discrete tile in a [`FootprintMosaic`] (`GEO-04`).
#[derive(Debug, Clone, PartialEq)]
pub enum MosaicTileGeometry {
    CameraViewingCone(ViewingCone2D),
    ImageryFootprint {
        bounds: GeoBoundingBoxWgs84,
        ground_sample_distance_m: f64,
    },
}

/// Discrete imagery footprint or camera viewing cone tile (`GEO-04`, `A04`).
#[derive(Debug, Clone, PartialEq)]
pub struct MosaicTile {
    pub tile_id: OpaqueId,
    pub source_id: OpaqueId,
    pub artifact_version: VersionId,
    pub status: ModalitySourceStatus,
    pub geometry: MosaicTileGeometry,
    pub calibration: TileCalibrationState,
    pub clock_alignment: ClockAlignmentStatus,
}

/// Enforces that imagery and camera views remain a discrete mosaic rather than a
/// fabricated continuous scene (`GEO-04`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MosaicPresentationMode {
    DiscreteFootprintAndConeMosaicOnly,
}

/// Confidence and caveats for a pairwise cross-view alignment (`GEO-04`, `A13`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrossViewAlignmentStatus {
    /// Both views have verified calibration at capture and synchronized clocks.
    CalibratedAndClockAligned { combined_clock_uncertainty_ms: u64 },
    /// Alignment confidence is degraded because clock sync was lost or calibration
    /// was invalidated/approximate (`GEO-04`, `docs/architecture.md` §3.1).
    DegradedAlignment { reasons: Vec<String> },
}

/// Cross-view alignment record between two tiles in a [`FootprintMosaic`] (`GEO-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossViewAlignment {
    pub tile_a_id: OpaqueId,
    pub tile_b_id: OpaqueId,
    pub correspondence_assumption: String,
    pub status: CrossViewAlignmentStatus,
}

/// Navigable discrete mosaic of satellite footprints and camera viewing cones (`GEO-04`, `A04`).
#[derive(Debug, Clone, PartialEq)]
pub struct FootprintMosaic {
    presentation_mode: MosaicPresentationMode,
    limits: GeoWorkspaceLimits,
    tiles: BTreeMap<OpaqueId, MosaicTile>,
}

impl FootprintMosaic {
    #[must_use]
    pub fn new(limits: GeoWorkspaceLimits) -> Self {
        Self {
            presentation_mode: MosaicPresentationMode::DiscreteFootprintAndConeMosaicOnly,
            limits,
            tiles: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn presentation_mode(&self) -> MosaicPresentationMode {
        self.presentation_mode
    }

    #[must_use]
    pub const fn tiles(&self) -> &BTreeMap<OpaqueId, MosaicTile> {
        &self.tiles
    }

    pub fn add_tile(&mut self, tile: MosaicTile) -> Result<(), GeoWorkspaceError> {
        if self.tiles.len() >= self.limits.max_mosaic_tiles {
            return Err(GeoWorkspaceError::ResourceLimitExceeded("max_mosaic_tiles"));
        }
        if self.tiles.contains_key(&tile.tile_id) {
            return Err(GeoWorkspaceError::DuplicateMosaicTile(tile.tile_id));
        }
        if let MosaicTileGeometry::ImageryFootprint {
            ground_sample_distance_m,
            ..
        } = tile.geometry
        {
            validate_positive_distance("ground_sample_distance_m", ground_sample_distance_m)?;
        }
        self.tiles.insert(tile.tile_id.clone(), tile);
        Ok(())
    }

    /// Returns the IDs of tiles whose footprint or viewing cone covers `point`.
    #[must_use]
    pub fn tiles_covering_point(&self, point: GeoPointWgs84) -> Vec<OpaqueId> {
        self.tiles
            .values()
            .filter(|tile| match &tile.geometry {
                MosaicTileGeometry::CameraViewingCone(cone) => cone.contains_point(point),
                MosaicTileGeometry::ImageryFootprint { bounds, .. } => bounds.contains_point(point),
            })
            .map(|tile| tile.tile_id.clone())
            .collect()
    }

    /// Evaluates cross-view alignment between two registered mosaic tiles, preserving
    /// calibration invalidation and clock-loss warnings (`GEO-04`, `A04`, `A13`).
    pub fn evaluate_cross_view_alignment(
        &self,
        tile_a_id: &OpaqueId,
        tile_b_id: &OpaqueId,
        correspondence_assumption: impl Into<String>,
    ) -> Result<CrossViewAlignment, GeoWorkspaceError> {
        let correspondence_assumption = correspondence_assumption.into();
        validate_text(
            "correspondence_assumption",
            &correspondence_assumption,
            self.limits.max_text_bytes,
        )?;
        let Some(tile_a) = self.tiles.get(tile_a_id) else {
            return Err(GeoWorkspaceError::UnknownMosaicTile(tile_a_id.clone()));
        };
        let Some(tile_b) = self.tiles.get(tile_b_id) else {
            return Err(GeoWorkspaceError::UnknownMosaicTile(tile_b_id.clone()));
        };

        let mut reasons = Vec::new();
        for tile in [tile_a, tile_b] {
            match &tile.calibration {
                TileCalibrationState::VerifiedAtCapture { .. } => {}
                TileCalibrationState::InvalidatedByCameraMovement {
                    movement_reason, ..
                } => {
                    reasons.push(format!(
                        "tile {} calibration invalidated by camera movement: {movement_reason}",
                        tile.tile_id
                    ));
                }
                TileCalibrationState::UncalibratedApproximate { nominal_fov_deg } => {
                    reasons.push(format!(
                        "tile {} uses uncalibrated nominal FOV ({nominal_fov_deg} deg)",
                        tile.tile_id
                    ));
                }
            }
            if let ClockAlignmentStatus::LostSyncReceiptTimeOnly { warning } = &tile.clock_alignment
            {
                reasons.push(format!(
                    "tile {} lost clock synchronization ({warning}); receipt time cannot substitute for capture time",
                    tile.tile_id
                ));
            }
            if matches!(
                tile.status,
                ModalitySourceStatus::StaleSnapshot { .. }
                    | ModalitySourceStatus::MissingHistory
                    | ModalitySourceStatus::Unavailable
            ) {
                reasons.push(format!(
                    "tile {} source status is {:?}",
                    tile.tile_id, tile.status
                ));
            }
        }

        let status = if reasons.is_empty() {
            let unc_a = match tile_a.clock_alignment {
                ClockAlignmentStatus::Synchronized { uncertainty_ms, .. } => uncertainty_ms,
                ClockAlignmentStatus::LostSyncReceiptTimeOnly { .. } => 0,
            };
            let unc_b = match tile_b.clock_alignment {
                ClockAlignmentStatus::Synchronized { uncertainty_ms, .. } => uncertainty_ms,
                ClockAlignmentStatus::LostSyncReceiptTimeOnly { .. } => 0,
            };
            CrossViewAlignmentStatus::CalibratedAndClockAligned {
                combined_clock_uncertainty_ms: unc_a.saturating_add(unc_b),
            }
        } else {
            CrossViewAlignmentStatus::DegradedAlignment { reasons }
        };

        Ok(CrossViewAlignment {
            tile_a_id: tile_a_id.clone(),
            tile_b_id: tile_b_id.clone(),
            correspondence_assumption,
            status,
        })
    }
}

/// Workspace view surfaces that share investigation context (`CASE-02`, `UX-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WorkspaceSurface {
    Globe,
    Sky,
    Documents,
    Media,
    Graph,
    MarketCharts,
    Report,
}

/// View-local filter state that must be disclosed when it differs from shared context (`CASE-02`).
#[derive(Debug, Clone, PartialEq)]
pub struct LocalViewFilter {
    pub area_override: Option<GeoBoundingBoxWgs84>,
    pub time_window_override_utc_ms: Option<(i64, i64)>,
    pub modality_filter: Option<BTreeSet<GeoModality>>,
    pub include_simulated: bool,
}

impl LocalViewFilter {
    /// Returns `true` if any local filter overrides the shared investigation context.
    #[must_use]
    pub fn diverges_from(
        &self,
        shared_area: GeoBoundingBoxWgs84,
        shared_time_window_utc_ms: (i64, i64),
    ) -> bool {
        let area_differs = self.area_override.is_some_and(|a| a != shared_area);
        let time_differs = self
            .time_window_override_utc_ms
            .is_some_and(|t| t != shared_time_window_utc_ms);
        let modality_restricted = self.modality_filter.is_some();
        area_differs || time_differs || modality_restricted || self.include_simulated
    }
}

/// Snapshot stored in route-back / route-forward navigation history (`CASE-02`, `UX-02`).
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceNavigationEntry {
    pub active_surface: WorkspaceSurface,
    pub selected_area: GeoBoundingBoxWgs84,
    pub selected_time_window_utc_ms: (i64, i64),
    pub selected_event_id: Option<OpaqueId>,
    pub selected_evidence_ids: BTreeSet<OpaqueId>,
    pub navigation_note: String,
}

/// Shared cross-view investigation context with local-filter divergence disclosure
/// and reversible route-back navigation (`CASE-02`, `UX-02`).
#[derive(Debug, Clone, PartialEq)]
pub struct SharedWorkspaceContext {
    investigation_id: OpaqueId,
    limits: GeoWorkspaceLimits,
    current: WorkspaceNavigationEntry,
    local_filters: BTreeMap<WorkspaceSurface, LocalViewFilter>,
    route_back: Vec<WorkspaceNavigationEntry>,
    route_forward: Vec<WorkspaceNavigationEntry>,
}

impl SharedWorkspaceContext {
    pub fn new(
        investigation_id: OpaqueId,
        limits: GeoWorkspaceLimits,
        initial_surface: WorkspaceSurface,
        selected_area: GeoBoundingBoxWgs84,
        start_utc_ms: i64,
        end_utc_ms: i64,
        navigation_note: impl Into<String>,
    ) -> Result<Self, GeoWorkspaceError> {
        if start_utc_ms >= end_utc_ms {
            return Err(GeoWorkspaceError::InvalidTimeInterval {
                start_utc_ms,
                end_utc_ms,
            });
        }
        let navigation_note = navigation_note.into();
        validate_text("navigation_note", &navigation_note, limits.max_text_bytes)?;
        Ok(Self {
            investigation_id,
            limits,
            current: WorkspaceNavigationEntry {
                active_surface: initial_surface,
                selected_area,
                selected_time_window_utc_ms: (start_utc_ms, end_utc_ms),
                selected_event_id: None,
                selected_evidence_ids: BTreeSet::new(),
                navigation_note,
            },
            local_filters: BTreeMap::new(),
            route_back: Vec::new(),
            route_forward: Vec::new(),
        })
    }

    #[must_use]
    pub const fn investigation_id(&self) -> &OpaqueId {
        &self.investigation_id
    }

    #[must_use]
    pub const fn current(&self) -> &WorkspaceNavigationEntry {
        &self.current
    }

    #[must_use]
    pub fn route_back(&self) -> &[WorkspaceNavigationEntry] {
        &self.route_back
    }

    #[must_use]
    pub fn route_forward(&self) -> &[WorkspaceNavigationEntry] {
        &self.route_forward
    }

    /// Coordinates the active view, shared area/timeline, selected event, and dossier
    /// evidence while pushing the previous state onto `route_back` (`CASE-02`, `UX-02`).
    #[allow(clippy::too_many_arguments)]
    pub fn coordinate_event_selection(
        &mut self,
        target_surface: WorkspaceSurface,
        selected_area: GeoBoundingBoxWgs84,
        start_utc_ms: i64,
        end_utc_ms: i64,
        selected_event_id: Option<OpaqueId>,
        selected_evidence_ids: BTreeSet<OpaqueId>,
        navigation_note: impl Into<String>,
    ) -> Result<(), GeoWorkspaceError> {
        if start_utc_ms >= end_utc_ms {
            return Err(GeoWorkspaceError::InvalidTimeInterval {
                start_utc_ms,
                end_utc_ms,
            });
        }
        if self.route_back.len() >= self.limits.max_navigation_steps {
            return Err(GeoWorkspaceError::ResourceLimitExceeded(
                "max_navigation_steps",
            ));
        }
        let navigation_note = navigation_note.into();
        validate_text(
            "navigation_note",
            &navigation_note,
            self.limits.max_text_bytes,
        )?;

        let next = WorkspaceNavigationEntry {
            active_surface: target_surface,
            selected_area,
            selected_time_window_utc_ms: (start_utc_ms, end_utc_ms),
            selected_event_id,
            selected_evidence_ids,
            navigation_note,
        };
        let prev = std::mem::replace(&mut self.current, next);
        self.route_back.push(prev);
        self.route_forward.clear();
        Ok(())
    }

    /// Returns to the previous navigation state without losing forward history (`UX-02`).
    pub fn navigate_back(&mut self) -> Result<&WorkspaceNavigationEntry, GeoWorkspaceError> {
        let Some(prev) = self.route_back.pop() else {
            return Err(GeoWorkspaceError::EmptyNavigationRouteBack);
        };
        let cur = std::mem::replace(&mut self.current, prev);
        self.route_forward.push(cur);
        Ok(&self.current)
    }

    /// Steps forward to a state previously exited via [`Self::navigate_back`].
    pub fn navigate_forward(&mut self) -> Result<&WorkspaceNavigationEntry, GeoWorkspaceError> {
        let Some(next) = self.route_forward.pop() else {
            return Err(GeoWorkspaceError::EmptyNavigationRouteForward);
        };
        let cur = std::mem::replace(&mut self.current, next);
        self.route_back.push(cur);
        Ok(&self.current)
    }

    /// Sets or updates a view-local filter for `surface` (`CASE-02`).
    pub fn set_local_filter(
        &mut self,
        surface: WorkspaceSurface,
        filter: LocalViewFilter,
    ) -> Result<(), GeoWorkspaceError> {
        if let Some((start_utc_ms, end_utc_ms)) = filter.time_window_override_utc_ms
            && start_utc_ms >= end_utc_ms
        {
            return Err(GeoWorkspaceError::InvalidTimeInterval {
                start_utc_ms,
                end_utc_ms,
            });
        }
        self.local_filters.insert(surface, filter);
        Ok(())
    }

    /// Lists all workspace surfaces whose local filters currently diverge from the
    /// shared investigation context (`CASE-02`).
    #[must_use]
    pub fn surfaces_with_divergent_local_filters(
        &self,
    ) -> Vec<(WorkspaceSurface, &LocalViewFilter)> {
        self.local_filters
            .iter()
            .filter(|(_, f)| {
                f.diverges_from(
                    self.current.selected_area,
                    self.current.selected_time_window_utc_ms,
                )
            })
            .map(|(s, f)| (*s, f))
            .collect()
    }
}
