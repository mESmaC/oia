//! Reversible, source-aware event clustering with distinct evidence counts,
//! lineage deduplication, mixed-stance cluster accounting, and non-destructive
//! revision/reassessment tracking (`GEO-03`, `A04`, `#23`).
//!
//! ## Scope and status (`AGENTS.md` §3, §4)
//!
//! - **Implemented (narrowed per PR #102 scope review):**
//!   - [`EventClusterLedger`] providing editable, append-only, reversible event
//!     clustering (`register_item_with_initial_cluster`, `merge_clusters`,
//!     `split_cluster`, `revert_decision`, `revise_item`, `reassess_cluster`) with
//!     immutable [`VersionId`]s and preserved [`ClusterDecisionRecord`] audit history.
//!   - Reuses [`oia_news_claims::LineageRelationKind`] (`Quotes`, `Republishes`,
//!     `Supersedes`, `Supports`) and [`oia_news_claims::ClaimAssessmentState`]
//!     together with [`oia_domain::witness::AccountOrigin`] (`Reported` vs
//!     `Simulated`).
//!   - Computes [`DistinctEvidenceCounts`] keeping `article_count`,
//!     `account_count`, `sensor_observation_count`, `independent_source_count`,
//!     `supportive_independent_source_count`, `contradictory_independent_source_count`,
//!     `mixed_stance_independent_source_count`, `proposed_event_count`, and
//!     `simulated_item_count` strictly separate.
//!   - Collapses shared `source_id` (across an item and its collapsing lineage
//!     chain), `Republishes`/`Quotes`/`Supersedes` lineage chains, and identical
//!     content hashes so syndicated reprints never inflate
//!     `independent_source_count`, while separating mixed-stance source classes
//!     (`MixedStanceSourceClass`) so deduplication never inflates apparent
//!     support (`AGENTS.md` §2).
//!   - Preserves historical `CalendarDateOnly` records without converting them to
//!     midnight UTC timestamps: temporal gap comparison between a `CalendarDateOnly`
//!     record and an instant/interval yields
//!     [`TemporalComparisonOutcome::CalendarDateOnlyCannotResolveSubDayGap`]
//!     with a timezone-safe lower bound (`raw_diff.saturating_sub(2)`)
//!     rather than fabricating a millisecond offset (`docs/architecture.md` §3.1).
//!   - Propagates reassessment across direct and transitive lineage dependents
//!     into side-table annotations (`cluster_reassessment_reasons` /
//!     `cluster_needs_reassessment`) so immutable [`ProposedEventCluster`]
//!     snapshots in `archived_cluster_versions` never change in place (`AGENTS.md` §4).
//! - **Excluded from this slice (owned by other work packages):**
//!   - Shared WGS84 coordinate, bounding-box, vertical-datum, and scientific time
//!     primitives belong to `oia-time-geo` (`#12`) and are not defined here.
//!     [`ClusteringTimePrecision`] and [`LocationQualification`] are provisional
//!     clustering-boundary types pending `#12` consolidation.
//!   - Weather, aircraft/maritime track interpolation, and sensor coverage
//!     envelopes belong to `#24`.
//!   - Shared workspace shell state, active surface selection, local view filter
//!     bars, and route-back navigation are deferred to `oia-app` (`#16`) and
//!     shared application services (`#14`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};
use oia_news_claims::{ClaimAssessmentState, LineageRelationKind};

/// Maximum UTF-8 byte length for labels, property keys/values, and rationales.
pub const DEFAULT_MAX_TEXT_BYTES: usize = 4_096;
/// Default maximum evidence items in a single [`EventClusterLedger`].
pub const DEFAULT_MAX_EVIDENCE_ITEMS: usize = 1_024;
/// Default maximum active clusters in a single [`EventClusterLedger`].
pub const DEFAULT_MAX_CLUSTERS: usize = 512;
/// Default maximum retained split/merge/revert/reassess decision history entries.
pub const DEFAULT_MAX_DECISION_HISTORY: usize = 512;
/// Default maximum properties and lineage links per evidence item.
pub const DEFAULT_MAX_PROPERTIES_PER_ITEM: usize = 64;
/// Default maximum retained revisions per evidence item.
pub const DEFAULT_MAX_REVISIONS_PER_ITEM: usize = 64;
/// Default maximum retained reassessment reasons per cluster version.
pub const DEFAULT_MAX_REASSESSMENT_REASONS_PER_CLUSTER: usize = 256;
/// Maximum allowed pre-computed pairwise distance or uncertainty in meters (`20,000` km).
pub const MAX_DISTANCE_M: f64 = 20_000_000.0;

/// Which resource limit was zero or exceeded in an [`EventClusterLedger`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterLimitKind {
    TextBytes,
    EvidenceItems,
    Clusters,
    DecisionHistory,
    PropertiesPerItem,
    LineageLinksPerItem,
    RevisionsPerItem,
    ReassessmentReasonsPerCluster,
}

impl fmt::Display for ClusterLimitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TextBytes => f.write_str("max_text_bytes"),
            Self::EvidenceItems => f.write_str("max_evidence_items"),
            Self::Clusters => f.write_str("max_clusters"),
            Self::DecisionHistory => f.write_str("max_decision_history"),
            Self::PropertiesPerItem => f.write_str("max_properties_per_item"),
            Self::LineageLinksPerItem => f.write_str("max_lineage_links_per_item"),
            Self::RevisionsPerItem => f.write_str("max_revisions_per_item"),
            Self::ReassessmentReasonsPerCluster => {
                f.write_str("max_reassessment_reasons_per_cluster")
            }
        }
    }
}

/// Configurable resource budgets for [`EventClusterLedger`] (`AGENTS.md` §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClusterLimits {
    max_text_bytes: usize,
    max_evidence_items: usize,
    max_clusters: usize,
    max_decision_history: usize,
    max_properties_per_item: usize,
    max_revisions_per_item: usize,
    max_reassessment_reasons_per_cluster: usize,
}

impl ClusterLimits {
    /// Creates validated resource budgets; rejects zero in any field.
    pub fn new(
        max_text_bytes: usize,
        max_evidence_items: usize,
        max_clusters: usize,
        max_decision_history: usize,
        max_properties_per_item: usize,
    ) -> Result<Self, GeoClusteringError> {
        Self::with_reassessment_limits(
            max_text_bytes,
            max_evidence_items,
            max_clusters,
            max_decision_history,
            max_properties_per_item,
            DEFAULT_MAX_REVISIONS_PER_ITEM,
            DEFAULT_MAX_REASSESSMENT_REASONS_PER_CLUSTER,
        )
    }

    /// Creates validated resource budgets with an explicit per-item revision limit
    /// and a matching default reassessment-reason limit (`max_revisions_per_item * 4`).
    pub fn with_revision_limit(
        max_text_bytes: usize,
        max_evidence_items: usize,
        max_clusters: usize,
        max_decision_history: usize,
        max_properties_per_item: usize,
        max_revisions_per_item: usize,
    ) -> Result<Self, GeoClusteringError> {
        Self::with_reassessment_limits(
            max_text_bytes,
            max_evidence_items,
            max_clusters,
            max_decision_history,
            max_properties_per_item,
            max_revisions_per_item,
            max_revisions_per_item.saturating_mul(4),
        )
    }

    /// Creates validated resource budgets including explicit per-item revision and
    /// per-cluster reassessment-reason caps.
    pub fn with_reassessment_limits(
        max_text_bytes: usize,
        max_evidence_items: usize,
        max_clusters: usize,
        max_decision_history: usize,
        max_properties_per_item: usize,
        max_revisions_per_item: usize,
        max_reassessment_reasons_per_cluster: usize,
    ) -> Result<Self, GeoClusteringError> {
        if max_text_bytes == 0 {
            return Err(GeoClusteringError::ZeroLimit(ClusterLimitKind::TextBytes));
        }
        if max_evidence_items == 0 {
            return Err(GeoClusteringError::ZeroLimit(
                ClusterLimitKind::EvidenceItems,
            ));
        }
        if max_clusters == 0 {
            return Err(GeoClusteringError::ZeroLimit(ClusterLimitKind::Clusters));
        }
        if max_decision_history == 0 {
            return Err(GeoClusteringError::ZeroLimit(
                ClusterLimitKind::DecisionHistory,
            ));
        }
        if max_properties_per_item == 0 {
            return Err(GeoClusteringError::ZeroLimit(
                ClusterLimitKind::PropertiesPerItem,
            ));
        }
        if max_revisions_per_item == 0 {
            return Err(GeoClusteringError::ZeroLimit(
                ClusterLimitKind::RevisionsPerItem,
            ));
        }
        if max_reassessment_reasons_per_cluster == 0 {
            return Err(GeoClusteringError::ZeroLimit(
                ClusterLimitKind::ReassessmentReasonsPerCluster,
            ));
        }
        Ok(Self {
            max_text_bytes,
            max_evidence_items,
            max_clusters,
            max_decision_history,
            max_properties_per_item,
            max_revisions_per_item,
            max_reassessment_reasons_per_cluster,
        })
    }

    #[must_use]
    pub const fn max_text_bytes(self) -> usize {
        self.max_text_bytes
    }

    #[must_use]
    pub const fn max_evidence_items(self) -> usize {
        self.max_evidence_items
    }

    #[must_use]
    pub const fn max_clusters(self) -> usize {
        self.max_clusters
    }

    #[must_use]
    pub const fn max_decision_history(self) -> usize {
        self.max_decision_history
    }

    #[must_use]
    pub const fn max_properties_per_item(self) -> usize {
        self.max_properties_per_item
    }

    #[must_use]
    pub const fn max_revisions_per_item(self) -> usize {
        self.max_revisions_per_item
    }

    #[must_use]
    pub const fn max_reassessment_reasons_per_cluster(self) -> usize {
        self.max_reassessment_reasons_per_cluster
    }
}

impl Default for ClusterLimits {
    fn default() -> Self {
        Self {
            max_text_bytes: DEFAULT_MAX_TEXT_BYTES,
            max_evidence_items: DEFAULT_MAX_EVIDENCE_ITEMS,
            max_clusters: DEFAULT_MAX_CLUSTERS,
            max_decision_history: DEFAULT_MAX_DECISION_HISTORY,
            max_properties_per_item: DEFAULT_MAX_PROPERTIES_PER_ITEM,
            max_revisions_per_item: DEFAULT_MAX_REVISIONS_PER_ITEM,
            max_reassessment_reasons_per_cluster: DEFAULT_MAX_REASSESSMENT_REASONS_PER_CLUSTER,
        }
    }
}

/// Typed errors returned by [`EventClusterLedger`] operations.
#[derive(Debug, Clone, PartialEq)]
pub enum GeoClusteringError {
    ZeroLimit(ClusterLimitKind),
    BudgetExceeded {
        limit: ClusterLimitKind,
        actual: usize,
        max: usize,
    },
    EmptyTextField(&'static str),
    TextFieldTooLong {
        field: &'static str,
        len: usize,
        max: usize,
    },
    ControlCharacterInText(&'static str),
    InvalidDistanceOrRadius {
        field: &'static str,
        value: f64,
    },
    InvalidCalendarDate {
        year: i32,
        month: u8,
        day: u8,
    },
    InvalidTimeInterval {
        start_utc_ms: i64,
        end_utc_ms: i64,
    },
    DuplicateEvidenceItem(OpaqueId),
    UnknownEvidenceItem(OpaqueId),
    DuplicateClusterId(OpaqueId),
    UnknownClusterId(OpaqueId),
    DuplicateVersionId(VersionId),
    DuplicateDecisionId(OpaqueId),
    UnknownDecisionId(OpaqueId),
    DecisionAlreadyReverted(OpaqueId),
    CannotRevertRevertDecision(OpaqueId),
    DecisionNoLongerCurrent {
        decision_id: OpaqueId,
        missing_or_modified_cluster: OpaqueId,
    },
    RevertDestinationClusterInUse {
        decision_id: OpaqueId,
        conflicting_cluster_id: OpaqueId,
    },
    InvalidRevertVersionCount {
        decision_id: OpaqueId,
        expected: usize,
        supplied: usize,
    },
    InvalidClusterSplitPartition {
        cluster_id: OpaqueId,
        reason: &'static str,
    },
    SelfLineageReference(OpaqueId),
    PredecessorVersionMismatch {
        item_id: OpaqueId,
        expected: VersionId,
        supplied: VersionId,
    },
    OriginChangeForbidden {
        item_id: OpaqueId,
    },
}

impl fmt::Display for GeoClusteringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit(kind) => write!(f, "clustering limit {kind} must be >= 1"),
            Self::BudgetExceeded { limit, actual, max } => {
                write!(f, "clustering limit {limit} exceeded ({actual} > {max})")
            }
            Self::EmptyTextField(field) => write!(f, "{field} must not be empty"),
            Self::TextFieldTooLong { field, len, max } => {
                write!(f, "{field} byte length {len} exceeds maximum {max}")
            }
            Self::ControlCharacterInText(field) => {
                write!(f, "{field} contains disallowed control characters")
            }
            Self::InvalidDistanceOrRadius { field, value } => write!(
                f,
                "{field} ({value}) must be finite and within (0, {MAX_DISTANCE_M}]"
            ),
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
            Self::DuplicateEvidenceItem(id) => write!(f, "duplicate evidence item ID {id}"),
            Self::UnknownEvidenceItem(id) => write!(f, "unknown evidence item ID {id}"),
            Self::DuplicateClusterId(id) => write!(f, "duplicate cluster ID {id}"),
            Self::UnknownClusterId(id) => write!(f, "unknown cluster ID {id}"),
            Self::DuplicateVersionId(v) => write!(f, "version ID {v} was already used"),
            Self::DuplicateDecisionId(id) => write!(f, "decision ID {id} was already used"),
            Self::UnknownDecisionId(id) => write!(f, "unknown decision ID {id}"),
            Self::DecisionAlreadyReverted(id) => write!(f, "decision {id} was already reverted"),
            Self::CannotRevertRevertDecision(id) => {
                write!(f, "revert decision {id} cannot itself be reverted")
            }
            Self::DecisionNoLongerCurrent {
                decision_id,
                missing_or_modified_cluster,
            } => write!(
                f,
                "decision {decision_id} cannot be reverted directly because cluster {missing_or_modified_cluster} was modified by a later decision"
            ),
            Self::RevertDestinationClusterInUse {
                decision_id,
                conflicting_cluster_id,
            } => write!(
                f,
                "decision {decision_id} cannot be reverted because destination cluster ID {conflicting_cluster_id} is currently active"
            ),
            Self::InvalidRevertVersionCount {
                decision_id,
                expected,
                supplied,
            } => write!(
                f,
                "reverting decision {decision_id} requires {expected} new VersionId(s), got {supplied}"
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
            Self::PredecessorVersionMismatch {
                item_id,
                expected,
                supplied,
            } => write!(
                f,
                "item {item_id} revision must cite current version {expected}, got {supplied}"
            ),
            Self::OriginChangeForbidden { item_id } => write!(
                f,
                "item {item_id} revision cannot change evidence kind or simulated/reported origin"
            ),
        }
    }
}

impl std::error::Error for GeoClusteringError {}

fn validate_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), GeoClusteringError> {
    if value.trim().is_empty() {
        return Err(GeoClusteringError::EmptyTextField(field));
    }
    if value.len() > max_bytes {
        return Err(GeoClusteringError::TextFieldTooLong {
            field,
            len: value.len(),
            max: max_bytes,
        });
    }
    if value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
    {
        return Err(GeoClusteringError::ControlCharacterInText(field));
    }
    Ok(())
}

fn validate_positive_distance(field: &'static str, value: f64) -> Result<f64, GeoClusteringError> {
    if !value.is_finite() || value <= 0.0 || value > MAX_DISTANCE_M {
        return Err(GeoClusteringError::InvalidDistanceOrRadius { field, value });
    }
    Ok(value)
}

/// Qualification required when a historical account is known only through a
/// later secondary or archival source (`GEO-01`, `A04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EarliestKnownSourceQualification {
    earliest_source_id: OpaqueId,
    earliest_source_version: VersionId,
    earliest_publication_date_text: String,
    gap_after_reported_event_note: String,
}

impl EarliestKnownSourceQualification {
    pub fn new(
        earliest_source_id: OpaqueId,
        earliest_source_version: VersionId,
        earliest_publication_date_text: impl Into<String>,
        gap_after_reported_event_note: impl Into<String>,
        max_text_bytes: usize,
    ) -> Result<Self, GeoClusteringError> {
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

    fn validate_against_limit(&self, max_text_bytes: usize) -> Result<(), GeoClusteringError> {
        validate_text(
            "earliest_publication_date_text",
            &self.earliest_publication_date_text,
            max_text_bytes,
        )?;
        validate_text(
            "gap_after_reported_event_note",
            &self.gap_after_reported_event_note,
            max_text_bytes,
        )?;
        Ok(())
    }

    #[must_use]
    pub const fn earliest_source_id(&self) -> &OpaqueId {
        &self.earliest_source_id
    }

    #[must_use]
    pub const fn earliest_source_version(&self) -> &VersionId {
        &self.earliest_source_version
    }

    #[must_use]
    pub fn earliest_publication_date_text(&self) -> &str {
        &self.earliest_publication_date_text
    }

    #[must_use]
    pub fn gap_after_reported_event_note(&self) -> &str {
        &self.gap_after_reported_event_note
    }
}

/// Provisional clustering time precision representation pending `#12` (`oia-time-geo`)
/// scientific time and calendar-date consolidation (`GEO-01`, `docs/architecture.md` §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClusteringTimePrecision {
    /// Sub-day instant anchored in UTC milliseconds since the Unix epoch.
    ExactInstant { utc_ms: i64 },
    /// Sub-day interval `[start_utc_ms, end_utc_ms]` with optional witness clock uncertainty note.
    BoundedInterval {
        start_utc_ms: i64,
        end_utc_ms: i64,
        witness_uncertainty_note: Option<String>,
    },
    /// Calendar date only (`YYYY-MM-DD`). Never converted to `00:00:00Z` (`docs/architecture.md` §3.1).
    CalendarDateOnly { year: i32, month: u8, day: u8 },
}

/// Outcome of comparing two [`ReportedClusteringTime`] values (`GEO-01`, `GEO-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalComparisonOutcome {
    /// Both items have sub-day UTC bounds; gap is `0` if overlapping, otherwise milliseconds.
    ExactSubDayGapMs(u64),
    /// Both items are `CalendarDateOnly`; separation is measured in whole calendar days.
    CalendarDateSeparationDays(u64),
    /// One item is `CalendarDateOnly` and the other is a sub-day timestamp or interval;
    /// sub-day separation cannot be computed without fabricating midnight UTC.
    /// `minimum_whole_day_separation` is a timezone-safe lower bound (`raw_diff.saturating_sub(2)`).
    CalendarDateOnlyCannotResolveSubDayGap { minimum_whole_day_separation: u64 },
}

/// Reported time for an evidence item participating in clustering (`GEO-01`, `GEO-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportedClusteringTime {
    reported_local_text: String,
    timezone_description: String,
    precision: ClusteringTimePrecision,
    earliest_known_source: Option<EarliestKnownSourceQualification>,
}

impl ReportedClusteringTime {
    pub fn exact_instant(
        reported_local_text: impl Into<String>,
        timezone_description: impl Into<String>,
        utc_ms: i64,
        earliest_known_source: Option<EarliestKnownSourceQualification>,
        max_text_bytes: usize,
    ) -> Result<Self, GeoClusteringError> {
        let candidate = Self {
            reported_local_text: reported_local_text.into(),
            timezone_description: timezone_description.into(),
            precision: ClusteringTimePrecision::ExactInstant { utc_ms },
            earliest_known_source,
        };
        candidate.validate_against_limit(max_text_bytes)?;
        Ok(candidate)
    }

    pub fn bounded_interval(
        reported_local_text: impl Into<String>,
        timezone_description: impl Into<String>,
        start_utc_ms: i64,
        end_utc_ms: i64,
        witness_uncertainty_note: Option<String>,
        earliest_known_source: Option<EarliestKnownSourceQualification>,
        max_text_bytes: usize,
    ) -> Result<Self, GeoClusteringError> {
        let candidate = Self {
            reported_local_text: reported_local_text.into(),
            timezone_description: timezone_description.into(),
            precision: ClusteringTimePrecision::BoundedInterval {
                start_utc_ms,
                end_utc_ms,
                witness_uncertainty_note,
            },
            earliest_known_source,
        };
        candidate.validate_against_limit(max_text_bytes)?;
        Ok(candidate)
    }

    /// Constructs a calendar-date-only record without converting it to midnight UTC.
    pub fn calendar_date_only(
        reported_local_text: impl Into<String>,
        timezone_description: impl Into<String>,
        year: i32,
        month: u8,
        day: u8,
        earliest_known_source: Option<EarliestKnownSourceQualification>,
        max_text_bytes: usize,
    ) -> Result<Self, GeoClusteringError> {
        let candidate = Self {
            reported_local_text: reported_local_text.into(),
            timezone_description: timezone_description.into(),
            precision: ClusteringTimePrecision::CalendarDateOnly { year, month, day },
            earliest_known_source,
        };
        candidate.validate_against_limit(max_text_bytes)?;
        Ok(candidate)
    }

    fn validate_against_limit(&self, max_text_bytes: usize) -> Result<(), GeoClusteringError> {
        validate_text(
            "reported_local_text",
            &self.reported_local_text,
            max_text_bytes,
        )?;
        validate_text(
            "timezone_description",
            &self.timezone_description,
            max_text_bytes,
        )?;
        if let Some(ref qual) = self.earliest_known_source {
            qual.validate_against_limit(max_text_bytes)?;
        }
        match &self.precision {
            ClusteringTimePrecision::ExactInstant { .. } => Ok(()),
            ClusteringTimePrecision::BoundedInterval {
                start_utc_ms,
                end_utc_ms,
                witness_uncertainty_note,
            } => {
                if start_utc_ms >= end_utc_ms {
                    return Err(GeoClusteringError::InvalidTimeInterval {
                        start_utc_ms: *start_utc_ms,
                        end_utc_ms: *end_utc_ms,
                    });
                }
                if let Some(note) = witness_uncertainty_note {
                    validate_text("witness_uncertainty_note", note, max_text_bytes)?;
                }
                Ok(())
            }
            ClusteringTimePrecision::CalendarDateOnly { year, month, day } => {
                oia_time_geo::CalendarDate::new(*year, *month, *day).map_err(|_| {
                    GeoClusteringError::InvalidCalendarDate {
                        year: *year,
                        month: *month,
                        day: *day,
                    }
                })?;
                Ok(())
            }
        }
    }

    #[must_use]
    pub fn reported_local_text(&self) -> &str {
        &self.reported_local_text
    }

    #[must_use]
    pub fn timezone_description(&self) -> &str {
        &self.timezone_description
    }

    #[must_use]
    pub const fn precision(&self) -> &ClusteringTimePrecision {
        &self.precision
    }

    #[must_use]
    pub const fn earliest_known_source(&self) -> Option<&EarliestKnownSourceQualification> {
        self.earliest_known_source.as_ref()
    }

    #[must_use]
    pub const fn is_calendar_date_only(&self) -> bool {
        matches!(
            self.precision,
            ClusteringTimePrecision::CalendarDateOnly { .. }
        )
    }

    /// Compares two reported times without promoting `CalendarDateOnly` to midnight UTC.
    #[must_use]
    pub fn compare_temporal_separation(&self, other: &Self) -> TemporalComparisonOutcome {
        match (&self.precision, &other.precision) {
            (
                ClusteringTimePrecision::CalendarDateOnly {
                    year: y1,
                    month: m1,
                    day: d1,
                },
                ClusteringTimePrecision::CalendarDateOnly {
                    year: y2,
                    month: m2,
                    day: d2,
                },
            ) => {
                let days1 = oia_time_geo::days_from_civil(i64::from(*y1), *m1, *d1);
                let days2 = oia_time_geo::days_from_civil(i64::from(*y2), *m2, *d2);
                TemporalComparisonOutcome::CalendarDateSeparationDays(days1.abs_diff(days2))
            }
            (
                ClusteringTimePrecision::CalendarDateOnly { year, month, day },
                ClusteringTimePrecision::ExactInstant { utc_ms },
            )
            | (
                ClusteringTimePrecision::ExactInstant { utc_ms },
                ClusteringTimePrecision::CalendarDateOnly { year, month, day },
            ) => {
                let cal_day = oia_time_geo::days_from_civil(i64::from(*year), *month, *day);
                let instant_utc_day = utc_ms.div_euclid(86_400_000);
                // Because local timezone offset may shift the UTC date by up to +14/-12 hours
                // (1 day) and a calendar date spans an entire 24-hour day (1 day), a valid
                // lower bound on whole-day separation requires subtracting 2 from the raw
                // UTC-day difference (`raw_diff.saturating_sub(2)`).
                let raw_diff = cal_day.abs_diff(instant_utc_day);
                TemporalComparisonOutcome::CalendarDateOnlyCannotResolveSubDayGap {
                    minimum_whole_day_separation: raw_diff.saturating_sub(2),
                }
            }
            (
                ClusteringTimePrecision::CalendarDateOnly { year, month, day },
                ClusteringTimePrecision::BoundedInterval {
                    start_utc_ms,
                    end_utc_ms,
                    ..
                },
            )
            | (
                ClusteringTimePrecision::BoundedInterval {
                    start_utc_ms,
                    end_utc_ms,
                    ..
                },
                ClusteringTimePrecision::CalendarDateOnly { year, month, day },
            ) => {
                let cal_day = oia_time_geo::days_from_civil(i64::from(*year), *month, *day);
                let start_day = start_utc_ms.div_euclid(86_400_000);
                let end_day = end_utc_ms.saturating_sub(1).div_euclid(86_400_000);
                let raw_diff = if cal_day < start_day {
                    start_day.abs_diff(cal_day)
                } else if cal_day > end_day {
                    cal_day.abs_diff(end_day)
                } else {
                    0
                };
                TemporalComparisonOutcome::CalendarDateOnlyCannotResolveSubDayGap {
                    minimum_whole_day_separation: raw_diff.saturating_sub(2),
                }
            }

            (
                ClusteringTimePrecision::ExactInstant { utc_ms: a_ms },
                ClusteringTimePrecision::ExactInstant { utc_ms: b_ms },
            ) => TemporalComparisonOutcome::ExactSubDayGapMs(a_ms.abs_diff(*b_ms)),
            (
                ClusteringTimePrecision::ExactInstant { utc_ms },
                ClusteringTimePrecision::BoundedInterval {
                    start_utc_ms,
                    end_utc_ms,
                    ..
                },
            )
            | (
                ClusteringTimePrecision::BoundedInterval {
                    start_utc_ms,
                    end_utc_ms,
                    ..
                },
                ClusteringTimePrecision::ExactInstant { utc_ms },
            ) => {
                let gap_ms = if *utc_ms < *start_utc_ms {
                    start_utc_ms.abs_diff(*utc_ms)
                } else if *utc_ms > *end_utc_ms {
                    utc_ms.abs_diff(*end_utc_ms)
                } else {
                    0
                };
                TemporalComparisonOutcome::ExactSubDayGapMs(gap_ms)
            }
            (
                ClusteringTimePrecision::BoundedInterval {
                    start_utc_ms: a_start,
                    end_utc_ms: a_end,
                    ..
                },
                ClusteringTimePrecision::BoundedInterval {
                    start_utc_ms: b_start,
                    end_utc_ms: b_end,
                    ..
                },
            ) => {
                let gap_ms = if a_end < b_start {
                    b_start.abs_diff(*a_end)
                } else if b_end < a_start {
                    a_start.abs_diff(*b_end)
                } else {
                    0
                };
                TemporalComparisonOutcome::ExactSubDayGapMs(gap_ms)
            }
        }
    }
}

/// Provisional location qualification kind for an evidence item pending `#12`
/// (`oia-time-geo`) shared coordinate/uncertainty consolidation (`GEO-01`, `A04`).
#[derive(Debug, Clone, PartialEq)]
pub enum LocationQualificationKind {
    /// Surveyed or instrument-measured location; `None` uncertainty means unknown, never zero.
    SurveyedLocation {
        location_ref: OpaqueId,
        horizontal_uncertainty_m: Option<f64>,
    },
    /// Gazetteer or named-place centroid; always carries a positive uncertainty radius
    /// so it never masquerades as a surveyed point (`docs/architecture.md` §3.2).
    ApproximateGazetteerCentroid {
        place_label: String,
        radius_uncertainty_m: f64,
    },
    /// Coarse bounded area or corridor description.
    BoundedRegionDescription { region_label: String },
}

/// Validated location qualification for an evidence item participating in event
/// clustering (`GEO-01`, `A04`). Provisional pending `#12` (`oia-time-geo`).
///
/// The inner [`LocationQualificationKind`] is private so callers cannot construct
/// unvalidated `NaN` or negative uncertainty radii (`AGENTS.md` §5).
#[derive(Debug, Clone, PartialEq)]
pub struct LocationQualification {
    kind: LocationQualificationKind,
}

impl LocationQualification {
    pub fn surveyed(
        location_ref: OpaqueId,
        horizontal_uncertainty_m: Option<f64>,
    ) -> Result<Self, GeoClusteringError> {
        if let Some(u) = horizontal_uncertainty_m {
            validate_positive_distance("horizontal_uncertainty_m", u)?;
        }
        Ok(Self {
            kind: LocationQualificationKind::SurveyedLocation {
                location_ref,
                horizontal_uncertainty_m,
            },
        })
    }

    pub fn gazetteer_centroid(
        place_label: impl Into<String>,
        radius_uncertainty_m: f64,
        max_text_bytes: usize,
    ) -> Result<Self, GeoClusteringError> {
        let place_label = place_label.into();
        validate_text("place_label", &place_label, max_text_bytes)?;
        let radius_uncertainty_m =
            validate_positive_distance("radius_uncertainty_m", radius_uncertainty_m)?;
        Ok(Self {
            kind: LocationQualificationKind::ApproximateGazetteerCentroid {
                place_label,
                radius_uncertainty_m,
            },
        })
    }

    pub fn bounded_region(
        region_label: impl Into<String>,
        max_text_bytes: usize,
    ) -> Result<Self, GeoClusteringError> {
        let region_label = region_label.into();
        validate_text("region_label", &region_label, max_text_bytes)?;
        Ok(Self {
            kind: LocationQualificationKind::BoundedRegionDescription { region_label },
        })
    }

    fn validate_against_limit(&self, max_text_bytes: usize) -> Result<(), GeoClusteringError> {
        match &self.kind {
            LocationQualificationKind::SurveyedLocation {
                horizontal_uncertainty_m,
                ..
            } => {
                if let Some(u) = horizontal_uncertainty_m {
                    validate_positive_distance("horizontal_uncertainty_m", *u)?;
                }
                Ok(())
            }
            LocationQualificationKind::ApproximateGazetteerCentroid {
                place_label,
                radius_uncertainty_m,
            } => {
                validate_text("place_label", place_label, max_text_bytes)?;
                validate_positive_distance("radius_uncertainty_m", *radius_uncertainty_m)?;
                Ok(())
            }
            LocationQualificationKind::BoundedRegionDescription { region_label } => {
                validate_text("region_label", region_label, max_text_bytes)
            }
        }
    }

    #[must_use]
    pub const fn kind(&self) -> &LocationQualificationKind {
        &self.kind
    }

    #[must_use]
    pub const fn is_approximate(&self) -> bool {
        !matches!(
            self.kind,
            LocationQualificationKind::SurveyedLocation { .. }
        )
    }
}

/// Classification of an evidence item participating in event clustering (`GEO-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeoEvidenceKind {
    /// Secondary or news article (counted in `article_count`).
    PublishedArticle,
    /// Primary witness account; `AccountOrigin::Simulated` items are excluded from
    /// observed `account_count` and `independent_source_count` (`AGENTS.md` §2).
    WitnessAccount { origin: AccountOrigin },
    /// Sensor, radar, or optical observation; `AccountOrigin::Simulated` items are
    /// excluded from observed `sensor_observation_count` and `independent_source_count`.
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

/// Lineage link reusing [`oia_news_claims::LineageRelationKind`] (`GEO-03`, `AGENTS.md` §2, §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceLineageLink {
    relation_kind: LineageRelationKind,
    upstream_item_id: OpaqueId,
}

impl EvidenceLineageLink {
    pub const fn new(relation_kind: LineageRelationKind, upstream_item_id: OpaqueId) -> Self {
        Self {
            relation_kind,
            upstream_item_id,
        }
    }

    #[must_use]
    pub const fn relation_kind(&self) -> LineageRelationKind {
        self.relation_kind
    }

    #[must_use]
    pub const fn upstream_item_id(&self) -> &OpaqueId {
        &self.upstream_item_id
    }

    /// Returns `true` when this relation derives from or replaces an upstream item
    /// (`Republishes`, `Quotes`, or `Supersedes`) and therefore must collapse into the
    /// same independent-source equivalence class.
    #[must_use]
    pub const fn collapses_independent_source(&self) -> bool {
        matches!(
            self.relation_kind,
            LineageRelationKind::Republishes
                | LineageRelationKind::Quotes
                | LineageRelationKind::Supersedes
        )
    }
}

/// Immutable versioned evidence item registered in an [`EventClusterLedger`] (`GEO-03`).
#[derive(Debug, Clone, PartialEq)]
pub struct GeoEvidenceItem {
    item_id: OpaqueId,
    version_id: VersionId,
    predecessor_version_id: Option<VersionId>,
    source_id: OpaqueId,
    content_hash: Option<String>,
    kind: GeoEvidenceKind,
    claim_assessment: ClaimAssessmentState,
    lineage_links: Vec<EvidenceLineageLink>,
    reported_time: ReportedClusteringTime,
    location_qualification: LocationQualification,
    reported_properties: BTreeMap<String, String>,
}

impl GeoEvidenceItem {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        item_id: OpaqueId,
        version_id: VersionId,
        source_id: OpaqueId,
        content_hash: Option<String>,
        kind: GeoEvidenceKind,
        claim_assessment: ClaimAssessmentState,
        lineage_links: Vec<EvidenceLineageLink>,
        reported_time: ReportedClusteringTime,
        location_qualification: LocationQualification,
        reported_properties: BTreeMap<String, String>,
        limits: ClusterLimits,
    ) -> Result<Self, GeoClusteringError> {
        let candidate = Self {
            item_id,
            version_id,
            predecessor_version_id: None,
            source_id,
            content_hash,
            kind,
            claim_assessment,
            lineage_links,
            reported_time,
            location_qualification,
            reported_properties,
        };
        candidate.validate_against_limits(limits)?;
        Ok(candidate)
    }

    fn validate_against_limits(&self, limits: ClusterLimits) -> Result<(), GeoClusteringError> {
        if self.reported_properties.len() > limits.max_properties_per_item {
            return Err(GeoClusteringError::BudgetExceeded {
                limit: ClusterLimitKind::PropertiesPerItem,
                actual: self.reported_properties.len(),
                max: limits.max_properties_per_item,
            });
        }
        if self.lineage_links.len() > limits.max_properties_per_item {
            return Err(GeoClusteringError::BudgetExceeded {
                limit: ClusterLimitKind::LineageLinksPerItem,
                actual: self.lineage_links.len(),
                max: limits.max_properties_per_item,
            });
        }
        for link in &self.lineage_links {
            if link.upstream_item_id == self.item_id {
                return Err(GeoClusteringError::SelfLineageReference(
                    self.item_id.clone(),
                ));
            }
        }
        if let Some(ref hash) = self.content_hash {
            validate_text("content_hash", hash, limits.max_text_bytes)?;
        }
        self.reported_time
            .validate_against_limit(limits.max_text_bytes)?;
        self.location_qualification
            .validate_against_limit(limits.max_text_bytes)?;
        for (k, v) in &self.reported_properties {
            validate_text("property_key", k, limits.max_text_bytes)?;
            validate_text("property_value", v, limits.max_text_bytes)?;
        }
        Ok(())
    }

    #[must_use]
    pub const fn item_id(&self) -> &OpaqueId {
        &self.item_id
    }

    #[must_use]
    pub const fn version_id(&self) -> &VersionId {
        &self.version_id
    }

    #[must_use]
    pub const fn predecessor_version_id(&self) -> Option<&VersionId> {
        self.predecessor_version_id.as_ref()
    }

    #[must_use]
    pub const fn source_id(&self) -> &OpaqueId {
        &self.source_id
    }

    #[must_use]
    pub fn content_hash(&self) -> Option<&str> {
        self.content_hash.as_deref()
    }

    #[must_use]
    pub const fn kind(&self) -> GeoEvidenceKind {
        self.kind
    }

    #[must_use]
    pub const fn claim_assessment(&self) -> ClaimAssessmentState {
        self.claim_assessment
    }

    #[must_use]
    pub fn lineage_links(&self) -> &[EvidenceLineageLink] {
        &self.lineage_links
    }

    #[must_use]
    pub const fn reported_time(&self) -> &ReportedClusteringTime {
        &self.reported_time
    }

    #[must_use]
    pub const fn location_qualification(&self) -> &LocationQualification {
        &self.location_qualification
    }

    #[must_use]
    pub const fn reported_properties(&self) -> &BTreeMap<String, String> {
        &self.reported_properties
    }
}

/// Matching criteria recorded on an event cluster (`GEO-03`).
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterMatchCriteria {
    max_proximity_m: f64,
    max_time_separation_ms: u64,
    max_calendar_day_separation: u64,
    required_property_keys: BTreeSet<String>,
}

impl ClusterMatchCriteria {
    pub fn new(
        max_proximity_m: f64,
        max_time_separation_ms: u64,
        max_calendar_day_separation: u64,
        required_property_keys: BTreeSet<String>,
    ) -> Result<Self, GeoClusteringError> {
        let max_proximity_m = validate_positive_distance("max_proximity_m", max_proximity_m)?;
        Ok(Self {
            max_proximity_m,
            max_time_separation_ms,
            max_calendar_day_separation,
            required_property_keys,
        })
    }

    fn validate_against_limits(&self, limits: ClusterLimits) -> Result<(), GeoClusteringError> {
        validate_positive_distance("max_proximity_m", self.max_proximity_m)?;
        if self.required_property_keys.len() > limits.max_properties_per_item {
            return Err(GeoClusteringError::BudgetExceeded {
                limit: ClusterLimitKind::PropertiesPerItem,
                actual: self.required_property_keys.len(),
                max: limits.max_properties_per_item,
            });
        }
        for key in &self.required_property_keys {
            validate_text("required_property_key", key, limits.max_text_bytes)?;
        }
        Ok(())
    }

    #[must_use]
    pub const fn max_proximity_m(&self) -> f64 {
        self.max_proximity_m
    }

    #[must_use]
    pub const fn max_time_separation_ms(&self) -> u64 {
        self.max_time_separation_ms
    }

    #[must_use]
    pub const fn max_calendar_day_separation(&self) -> u64 {
        self.max_calendar_day_separation
    }

    #[must_use]
    pub const fn required_property_keys(&self) -> &BTreeSet<String> {
        &self.required_property_keys
    }
}

/// Conflict or qualification surfaced within an event cluster (`GEO-03`, `A04`).
#[derive(Debug, Clone, PartialEq)]
pub enum ClusterConflict {
    /// Caller-supplied pairwise distance between two items exceeds `criteria.max_proximity_m`.
    SpatialSeparationExceedsThreshold {
        item_a: OpaqueId,
        item_b: OpaqueId,
        distance_m: f64,
        threshold_m: f64,
    },
    /// No pairwise distance was recorded between `item_a` and `item_b`, so spatial
    /// proximity could not be evaluated (`AGENTS.md` §2: coverage gaps must be explicit).
    SpatialSeparationNotEvaluated { item_a: OpaqueId, item_b: OpaqueId },
    /// Two sub-day items in the cluster exceed `criteria.max_time_separation_ms`.
    TemporalSeparationExceedsThreshold {
        item_a: OpaqueId,
        item_b: OpaqueId,
        gap_ms: u64,
        threshold_ms: u64,
    },
    /// Two calendar-date items exceed `criteria.max_calendar_day_separation`.
    CalendarDateSeparationExceedsThreshold {
        item_a: OpaqueId,
        item_b: OpaqueId,
        day_gap: u64,
        threshold_days: u64,
    },
    /// A `CalendarDateOnly` item is grouped with a sub-day timestamped item; sub-day
    /// gap cannot be resolved without inventing a midnight timestamp (`docs/architecture.md` §3.1).
    CalendarDateSubDayGapUnresolvable {
        item_a: OpaqueId,
        item_b: OpaqueId,
        minimum_whole_day_separation: u64,
    },
    /// Members report conflicting values for the same property key (e.g., `"shape"` or `"direction"`).
    PropertyMismatch {
        property_key: String,
        values_by_item: BTreeMap<OpaqueId, String>,
    },
    /// Member omits a property key listed in `criteria.required_property_keys`.
    MissingRequiredProperty {
        item_id: OpaqueId,
        property_key: String,
    },
    /// Member uses an approximate gazetteer centroid or bounded area rather than a surveyed point.
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
    /// from observed account, sensor, and independent-source counts.
    SimulatedEvidencePresent { item_id: OpaqueId },
    /// A reported item derives exclusively from a simulated upstream item via lineage,
    /// so it is excluded from observed `independent_source_count` (`AGENTS.md` §2).
    LineageAnchoredOnlyToSimulatedEvidence { item_id: OpaqueId },
    /// An independent-source equivalence class contains items with mixed or conflicting
    /// [`ClaimAssessmentState`] values (e.g., `Supported` and `Contradicted`), so it is
    /// excluded from `supportive_independent_source_count` (`AGENTS.md` §2).
    MixedStanceSourceClass {
        representative_item_id: OpaqueId,
        member_assessments: BTreeMap<OpaqueId, ClaimAssessmentState>,
    },
    /// Lineage edges form an unanchored citation cycle with no independent root.
    UnanchoredLineageCycle { cycle_members: Vec<OpaqueId> },
}

/// Strictly separate counts of articles, witness accounts, sensor observations,
/// independent sources, stance-partitioned independent sources, proposed events,
/// and simulated items (`GEO-03`, `A04`, `AGENTS.md` §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistinctEvidenceCounts {
    /// Total `PublishedArticle` items in the scope.
    pub article_count: usize,
    /// Observed (`AccountOrigin::Reported`) witness accounts in the scope.
    pub account_count: usize,
    /// Observed (`AccountOrigin::Reported`) sensor observations in the scope.
    pub sensor_observation_count: usize,
    /// Total anchored independent-source equivalence classes (after collapsing
    /// `Republishes`/`Quotes`/`Supersedes` lineage, shared `source_id`, and shared `content_hash`).
    pub independent_source_count: usize,
    /// Independent-source equivalence classes where every member has `ClaimAssessmentState::Supported`.
    /// Mixed-stance classes are never counted as supportive (`AGENTS.md` §2).
    pub supportive_independent_source_count: usize,
    /// Independent-source equivalence classes where every member has `ClaimAssessmentState::Contradicted`.
    pub contradictory_independent_source_count: usize,
    /// Independent-source equivalence classes whose members disagree in `ClaimAssessmentState`
    /// (e.g., one `Supported` and one `Contradicted` or `Disputed`).
    pub mixed_stance_independent_source_count: usize,
    /// Active proposed event clusters (`1` for a single cluster snapshot; `clusters().len()` ledger-wide).
    pub proposed_event_count: usize,
    /// Controlled simulation items (`AccountOrigin::Simulated`), excluded from all observed counts above.
    pub simulated_item_count: usize,
}

/// Immutable snapshot of a proposed event cluster (`GEO-03`).
///
/// Once created under a [`VersionId`], a `ProposedEventCluster` snapshot is never
/// mutated in place. Subsequent upstream item revisions or lineage updates are
/// recorded in [`EventClusterLedger::cluster_reassessment_reasons`] and carried
/// forward into successor cluster versions on merge, split, or revert until
/// resolved via [`EventClusterLedger::reassess_cluster`] (`AGENTS.md` §4).
#[derive(Debug, Clone, PartialEq)]
pub struct ProposedEventCluster {
    cluster_id: OpaqueId,
    version_id: VersionId,
    predecessor_version_ids: Vec<VersionId>,
    member_item_ids: BTreeSet<OpaqueId>,
    pinned_item_versions: BTreeMap<OpaqueId, VersionId>,
    criteria: ClusterMatchCriteria,
    conflicts: Vec<ClusterConflict>,
    counts: DistinctEvidenceCounts,
    needs_reassessment_at_issue: bool,
    reassessment_reasons_at_issue: Vec<String>,
    rationale: String,
}

impl ProposedEventCluster {
    #[must_use]
    pub const fn cluster_id(&self) -> &OpaqueId {
        &self.cluster_id
    }

    #[must_use]
    pub const fn version_id(&self) -> &VersionId {
        &self.version_id
    }

    #[must_use]
    pub fn predecessor_version_ids(&self) -> &[VersionId] {
        &self.predecessor_version_ids
    }

    #[must_use]
    pub const fn member_item_ids(&self) -> &BTreeSet<OpaqueId> {
        &self.member_item_ids
    }

    #[must_use]
    pub const fn pinned_item_versions(&self) -> &BTreeMap<OpaqueId, VersionId> {
        &self.pinned_item_versions
    }

    #[must_use]
    pub const fn criteria(&self) -> &ClusterMatchCriteria {
        &self.criteria
    }

    #[must_use]
    pub fn conflicts(&self) -> &[ClusterConflict] {
        &self.conflicts
    }

    #[must_use]
    pub const fn counts(&self) -> DistinctEvidenceCounts {
        self.counts
    }

    /// Returns `true` if this cluster version already had unresolved reassessment
    /// reasons carried forward from a predecessor version when the snapshot was issued.
    /// For live post-snapshot reassessment status, use
    /// [`EventClusterLedger::cluster_needs_reassessment`].
    #[must_use]
    pub const fn needs_reassessment_at_issue(&self) -> bool {
        self.needs_reassessment_at_issue
    }

    /// Returns the reassessment reasons carried on this frozen snapshot at issue time.
    /// For live post-snapshot reasons, use [`EventClusterLedger::cluster_reassessment_reasons`].
    #[must_use]
    pub fn reassessment_reasons_at_issue(&self) -> &[String] {
        &self.reassessment_reasons_at_issue
    }

    #[must_use]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }
}

/// Reversible cluster mutation kind (`GEO-03`).
#[derive(Debug, Clone, PartialEq)]
pub enum ClusterDecisionKind {
    Merge {
        source_cluster_a: Box<ProposedEventCluster>,
        source_cluster_b: Box<ProposedEventCluster>,
        merged_cluster_id: OpaqueId,
        merged_version_id: VersionId,
    },
    Split {
        original_cluster: Box<ProposedEventCluster>,
        left_cluster_id: OpaqueId,
        left_version_id: VersionId,
        right_cluster_id: OpaqueId,
        right_version_id: VersionId,
    },
    Reassess {
        prior_cluster: Box<ProposedEventCluster>,
        reassessed_version_id: VersionId,
        acknowledged_reasons: Vec<String>,
    },
    Revert {
        reverted_decision_id: OpaqueId,
        restored_cluster_versions: Vec<(OpaqueId, VersionId)>,
    },
}

/// Append-only audit record for an editable clustering decision (`GEO-03`).
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterDecisionRecord {
    decision_id: OpaqueId,
    actor: ActorId,
    kind: ClusterDecisionKind,
    rationale: String,
    reverted_by: Option<OpaqueId>,
}

impl ClusterDecisionRecord {
    #[must_use]
    pub const fn decision_id(&self) -> &OpaqueId {
        &self.decision_id
    }

    #[must_use]
    pub const fn actor(&self) -> &ActorId {
        &self.actor
    }

    #[must_use]
    pub const fn kind(&self) -> &ClusterDecisionKind {
        &self.kind
    }

    #[must_use]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }

    #[must_use]
    pub const fn reverted_by(&self) -> Option<&OpaqueId> {
        self.reverted_by.as_ref()
    }
}

fn canonical_pair(a: &OpaqueId, b: &OpaqueId) -> (OpaqueId, OpaqueId) {
    if a <= b {
        (a.clone(), b.clone())
    } else {
        (b.clone(), a.clone())
    }
}

/// Append-only, reversible event clustering ledger (`GEO-03`, `A04`).
#[derive(Debug, Clone, PartialEq)]
pub struct EventClusterLedger {
    schema_version: SchemaVersion,
    limits: ClusterLimits,
    items: BTreeMap<OpaqueId, GeoEvidenceItem>,
    item_version_history: BTreeMap<OpaqueId, Vec<GeoEvidenceItem>>,
    active_clusters: BTreeMap<OpaqueId, ProposedEventCluster>,
    archived_cluster_versions: BTreeMap<VersionId, ProposedEventCluster>,
    reassessment_by_cluster_version: BTreeMap<VersionId, Vec<String>>,
    used_versions: BTreeSet<VersionId>,
    pairwise_distances_m: BTreeMap<(OpaqueId, OpaqueId), f64>,
    decision_history: Vec<ClusterDecisionRecord>,
}

impl EventClusterLedger {
    #[must_use]
    pub fn new(limits: ClusterLimits) -> Self {
        Self {
            schema_version: SchemaVersion::V1,
            limits,
            items: BTreeMap::new(),
            item_version_history: BTreeMap::new(),
            active_clusters: BTreeMap::new(),
            archived_cluster_versions: BTreeMap::new(),
            reassessment_by_cluster_version: BTreeMap::new(),
            used_versions: BTreeSet::new(),
            pairwise_distances_m: BTreeMap::new(),
            decision_history: Vec::new(),
        }
    }

    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    #[must_use]
    pub const fn limits(&self) -> ClusterLimits {
        self.limits
    }

    #[must_use]
    pub const fn items(&self) -> &BTreeMap<OpaqueId, GeoEvidenceItem> {
        &self.items
    }

    /// Returns the full append-only revision history for every registered item (`AGENTS.md` §4).
    #[must_use]
    pub const fn item_version_history(&self) -> &BTreeMap<OpaqueId, Vec<GeoEvidenceItem>> {
        &self.item_version_history
    }

    #[must_use]
    pub const fn clusters(&self) -> &BTreeMap<OpaqueId, ProposedEventCluster> {
        &self.active_clusters
    }

    /// Returns all historical cluster versions retained across merges, splits, and reverts.
    /// Every archived snapshot is immutable and identical to the snapshot produced when
    /// its [`VersionId`] was minted.
    #[must_use]
    pub const fn archived_cluster_versions(&self) -> &BTreeMap<VersionId, ProposedEventCluster> {
        &self.archived_cluster_versions
    }

    /// Returns `true` if the active or archived cluster at `version_id` requires
    /// reassessment (either carried at snapshot creation or flagged after creation
    /// by a direct or transitive upstream item or distance update).
    #[must_use]
    pub fn cluster_version_needs_reassessment(&self, version_id: &VersionId) -> bool {
        self.reassessment_by_cluster_version
            .get(version_id)
            .is_some_and(|r| !r.is_empty())
            || self
                .archived_cluster_versions
                .get(version_id)
                .is_some_and(ProposedEventCluster::needs_reassessment_at_issue)
    }

    /// Returns `true` if the active cluster `cluster_id` requires reassessment.
    #[must_use]
    pub fn cluster_needs_reassessment(&self, cluster_id: &OpaqueId) -> bool {
        self.active_clusters
            .get(cluster_id)
            .is_some_and(|c| self.cluster_version_needs_reassessment(&c.version_id))
    }

    /// Returns the reassessment reasons for the active cluster `cluster_id`.
    #[must_use]
    pub fn cluster_reassessment_reasons(&self, cluster_id: &OpaqueId) -> &[String] {
        let Some(cluster) = self.active_clusters.get(cluster_id) else {
            return &[];
        };
        self.reassessment_by_cluster_version
            .get(&cluster.version_id)
            .map_or(cluster.reassessment_reasons_at_issue(), Vec::as_slice)
    }

    #[must_use]
    pub fn decision_history(&self) -> &[ClusterDecisionRecord] {
        &self.decision_history
    }

    /// Records a pre-computed pairwise distance in meters between two registered evidence items.
    ///
    /// If an active cluster already contains both `item_a` and `item_b`, recording or updating
    /// their pairwise distance flags that cluster for reassessment (`AGENTS.md` §4).
    pub fn record_pairwise_distance_m(
        &mut self,
        item_a: &OpaqueId,
        item_b: &OpaqueId,
        distance_m: f64,
    ) -> Result<(), GeoClusteringError> {
        if !self.items.contains_key(item_a) {
            return Err(GeoClusteringError::UnknownEvidenceItem(item_a.clone()));
        }
        if !self.items.contains_key(item_b) {
            return Err(GeoClusteringError::UnknownEvidenceItem(item_b.clone()));
        }
        if !distance_m.is_finite() || !(0.0..=MAX_DISTANCE_M).contains(&distance_m) {
            return Err(GeoClusteringError::InvalidDistanceOrRadius {
                field: "distance_m",
                value: distance_m,
            });
        }
        if item_a == item_b {
            return Ok(());
        }
        let normalized = if distance_m == 0.0 { 0.0 } else { distance_m };
        let pair = canonical_pair(item_a, item_b);
        let prior = self.pairwise_distances_m.get(&pair).copied();
        if prior == Some(normalized) {
            return Ok(());
        }

        // Check reassessment-reason budgets on any active cluster containing both items.
        let mut affected_versions = Vec::new();
        for cluster in self.active_clusters.values() {
            if cluster.member_item_ids.contains(item_a) && cluster.member_item_ids.contains(item_b)
            {
                let len = self
                    .reassessment_by_cluster_version
                    .get(&cluster.version_id)
                    .map_or(0, Vec::len);
                if len >= self.limits.max_reassessment_reasons_per_cluster {
                    return Err(GeoClusteringError::BudgetExceeded {
                        limit: ClusterLimitKind::ReassessmentReasonsPerCluster,
                        actual: len + 1,
                        max: self.limits.max_reassessment_reasons_per_cluster,
                    });
                }
                affected_versions.push(cluster.version_id.clone());
            }
        }

        self.pairwise_distances_m.insert(pair, normalized);
        for ver_id in affected_versions {
            let reason = match prior {
                Some(old_d) => format!(
                    "pairwise distance between {item_a} and {item_b} updated from {old_d}m to {normalized}m"
                ),
                None => format!(
                    "pairwise distance between {item_a} and {item_b} recorded as {normalized}m"
                ),
            };
            self.reassessment_by_cluster_version
                .entry(ver_id)
                .or_default()
                .push(reason);
        }
        Ok(())
    }

    /// Registers an evidence item and places it in its own initial cluster.
    ///
    /// Revalidates `item` and `criteria` against `self.limits` so caller-supplied
    /// constructor limits cannot bypass the ledger's resource budgets (`AGENTS.md` §5).
    /// Also checks whether any existing active cluster's members reference the newly
    /// registered item as a lineage upstream and flags those dependent clusters for
    /// reassessment (`AGENTS.md` §4).
    pub fn register_item_with_initial_cluster(
        &mut self,
        item: GeoEvidenceItem,
        initial_cluster_id: OpaqueId,
        initial_cluster_version: VersionId,
        criteria: ClusterMatchCriteria,
        rationale: impl Into<String>,
    ) -> Result<(), GeoClusteringError> {
        if self.items.len() >= self.limits.max_evidence_items {
            return Err(GeoClusteringError::BudgetExceeded {
                limit: ClusterLimitKind::EvidenceItems,
                actual: self.items.len() + 1,
                max: self.limits.max_evidence_items,
            });
        }
        if self.active_clusters.len() >= self.limits.max_clusters {
            return Err(GeoClusteringError::BudgetExceeded {
                limit: ClusterLimitKind::Clusters,
                actual: self.active_clusters.len() + 1,
                max: self.limits.max_clusters,
            });
        }
        item.validate_against_limits(self.limits)?;
        criteria.validate_against_limits(self.limits)?;
        if self.items.contains_key(&item.item_id) {
            return Err(GeoClusteringError::DuplicateEvidenceItem(item.item_id));
        }
        if self.active_clusters.contains_key(&initial_cluster_id) {
            return Err(GeoClusteringError::DuplicateClusterId(initial_cluster_id));
        }
        if self.used_versions.contains(&item.version_id) {
            return Err(GeoClusteringError::DuplicateVersionId(item.version_id));
        }
        if self.used_versions.contains(&initial_cluster_version)
            || item.version_id == initial_cluster_version
        {
            return Err(GeoClusteringError::DuplicateVersionId(
                initial_cluster_version,
            ));
        }
        let rationale = rationale.into();
        validate_text("rationale", &rationale, self.limits.max_text_bytes)?;

        // Check if any existing active cluster reaches `item.item_id` via lineage,
        // and verify reassessment-reason budgets before mutating ledger state.
        let dependent_existing = self.active_clusters_reaching_item(&item.item_id, None);
        for (_, ver_id) in &dependent_existing {
            let existing_len = self
                .reassessment_by_cluster_version
                .get(ver_id)
                .map_or(0, Vec::len);
            if existing_len >= self.limits.max_reassessment_reasons_per_cluster {
                return Err(GeoClusteringError::BudgetExceeded {
                    limit: ClusterLimitKind::ReassessmentReasonsPerCluster,
                    actual: existing_len + 1,
                    max: self.limits.max_reassessment_reasons_per_cluster,
                });
            }
        }

        let item_id = item.item_id.clone();
        let item_ver = item.version_id.clone();
        self.used_versions.insert(item.version_id.clone());
        self.used_versions.insert(initial_cluster_version.clone());
        self.item_version_history
            .insert(item_id.clone(), vec![item.clone()]);
        self.items.insert(item_id.clone(), item);

        for (_, ver_id) in dependent_existing {
            self.reassessment_by_cluster_version
                .entry(ver_id)
                .or_default()
                .push(format!(
                    "upstream lineage item {item_id} registered at version {item_ver}"
                ));
        }

        let mut members = BTreeSet::new();
        members.insert(item_id);
        let cluster = self.build_cluster_snapshot(
            initial_cluster_id.clone(),
            initial_cluster_version.clone(),
            Vec::new(),
            members,
            criteria,
            Vec::new(),
            rationale,
        )?;
        self.archived_cluster_versions
            .insert(initial_cluster_version, cluster.clone());
        self.active_clusters.insert(initial_cluster_id, cluster);
        Ok(())
    }

    /// Revises an existing evidence item under a new [`VersionId`], preserving all prior
    /// item versions and recording reassessment reasons for every active cluster that
    /// contains the item or depends on it transitively through lineage (`AGENTS.md` §4).
    ///
    /// Existing [`ProposedEventCluster`] snapshots (both active and archived) are never
    /// mutated in place; reassessment reasons are stored in side-table annotations
    /// (`cluster_needs_reassessment`, `cluster_reassessment_reasons`) and carried
    /// forward into successor cluster versions on merge, split, or revert until
    /// acknowledged via [`EventClusterLedger::reassess_cluster`].
    pub fn revise_item(
        &mut self,
        expected_predecessor_version: &VersionId,
        mut revised_item: GeoEvidenceItem,
        revision_reason: impl Into<String>,
    ) -> Result<Vec<OpaqueId>, GeoClusteringError> {
        let revision_reason = revision_reason.into();
        validate_text(
            "revision_reason",
            &revision_reason,
            self.limits.max_text_bytes,
        )?;
        revised_item.validate_against_limits(self.limits)?;
        if self.used_versions.contains(&revised_item.version_id) {
            return Err(GeoClusteringError::DuplicateVersionId(
                revised_item.version_id,
            ));
        }
        let Some(current) = self.items.get(&revised_item.item_id) else {
            return Err(GeoClusteringError::UnknownEvidenceItem(
                revised_item.item_id,
            ));
        };
        if &current.version_id != expected_predecessor_version {
            return Err(GeoClusteringError::PredecessorVersionMismatch {
                item_id: revised_item.item_id,
                expected: current.version_id.clone(),
                supplied: expected_predecessor_version.clone(),
            });
        }
        if current.kind != revised_item.kind {
            return Err(GeoClusteringError::OriginChangeForbidden {
                item_id: revised_item.item_id,
            });
        }
        let history_len = self
            .item_version_history
            .get(&revised_item.item_id)
            .map_or(0, Vec::len);
        if history_len >= self.limits.max_revisions_per_item {
            return Err(GeoClusteringError::BudgetExceeded {
                limit: ClusterLimitKind::RevisionsPerItem,
                actual: history_len + 1,
                max: self.limits.max_revisions_per_item,
            });
        }

        // Determine all active clusters affected either under the old or new item state
        // (direct membership or transitive lineage reachability).
        let item_id = revised_item.item_id.clone();
        let mut affected = self.active_clusters_reaching_item(&item_id, None);
        let affected_after = self.active_clusters_reaching_item(&item_id, Some(&revised_item));
        for (cid, ver) in affected_after {
            if !affected.iter().any(|(c, _)| c == &cid) {
                affected.push((cid, ver));
            }
        }
        for (_, ver_id) in &affected {
            let existing_len = self
                .reassessment_by_cluster_version
                .get(ver_id)
                .map_or(0, Vec::len);
            if existing_len >= self.limits.max_reassessment_reasons_per_cluster {
                return Err(GeoClusteringError::BudgetExceeded {
                    limit: ClusterLimitKind::ReassessmentReasonsPerCluster,
                    actual: existing_len + 1,
                    max: self.limits.max_reassessment_reasons_per_cluster,
                });
            }
        }

        revised_item.predecessor_version_id = Some(expected_predecessor_version.clone());
        let new_ver = revised_item.version_id.clone();
        self.used_versions.insert(new_ver.clone());
        if let Some(history) = self.item_version_history.get_mut(&item_id) {
            history.push(revised_item.clone());
        }
        self.items.insert(item_id.clone(), revised_item);

        let mut flagged_clusters = Vec::with_capacity(affected.len());
        for (cid, ver_id) in affected {
            let is_direct = self
                .active_clusters
                .get(&cid)
                .is_some_and(|c| c.member_item_ids.contains(&item_id));
            let msg = if is_direct {
                format!(
                    "member {item_id} revised from {expected_predecessor_version} to {new_ver}: {revision_reason}"
                )
            } else {
                format!(
                    "upstream lineage item {item_id} revised from {expected_predecessor_version} to {new_ver}: {revision_reason}"
                )
            };
            self.reassessment_by_cluster_version
                .entry(ver_id)
                .or_default()
                .push(msg);
            flagged_clusters.push(cid);
        }
        Ok(flagged_clusters)
    }

    /// Reassesses an active cluster against the latest ledger item versions and pairwise
    /// distances, minting a new [`VersionId`] with `needs_reassessment == false` while
    /// preserving the prior cluster snapshot in `archived_cluster_versions` and recording
    /// the acknowledged reassessment reasons in [`ClusterDecisionKind::Reassess`] (`AGENTS.md` §4).
    pub fn reassess_cluster(
        &mut self,
        cluster_id: &OpaqueId,
        new_version_id: VersionId,
        criteria: ClusterMatchCriteria,
        decision_id: OpaqueId,
        actor: ActorId,
        rationale: impl Into<String>,
    ) -> Result<&ProposedEventCluster, GeoClusteringError> {
        if self.decision_history.len() >= self.limits.max_decision_history {
            return Err(GeoClusteringError::BudgetExceeded {
                limit: ClusterLimitKind::DecisionHistory,
                actual: self.decision_history.len() + 1,
                max: self.limits.max_decision_history,
            });
        }
        criteria.validate_against_limits(self.limits)?;
        if self.used_versions.contains(&new_version_id) {
            return Err(GeoClusteringError::DuplicateVersionId(new_version_id));
        }
        if self
            .decision_history
            .iter()
            .any(|d| d.decision_id == decision_id)
        {
            return Err(GeoClusteringError::DuplicateDecisionId(decision_id));
        }
        let rationale = rationale.into();
        validate_text("rationale", &rationale, self.limits.max_text_bytes)?;

        let Some(prior) = self.active_clusters.get(cluster_id).cloned() else {
            return Err(GeoClusteringError::UnknownClusterId(cluster_id.clone()));
        };
        let acknowledged_reasons = self
            .reassessment_by_cluster_version
            .get(&prior.version_id)
            .cloned()
            .unwrap_or_else(|| prior.reassessment_reasons_at_issue.clone());

        let reassessed_snapshot = self.build_cluster_snapshot(
            cluster_id.clone(),
            new_version_id.clone(),
            vec![prior.version_id.clone()],
            prior.member_item_ids.clone(),
            criteria,
            Vec::new(),
            rationale.clone(),
        )?;

        self.used_versions.insert(new_version_id.clone());
        self.archived_cluster_versions
            .insert(new_version_id.clone(), reassessed_snapshot.clone());
        self.active_clusters
            .insert(cluster_id.clone(), reassessed_snapshot);
        self.decision_history.push(ClusterDecisionRecord {
            decision_id,
            actor,
            kind: ClusterDecisionKind::Reassess {
                prior_cluster: Box::new(prior),
                reassessed_version_id: new_version_id,
                acknowledged_reasons,
            },
            rationale,
            reverted_by: None,
        });
        self.active_clusters
            .get(cluster_id)
            .ok_or_else(|| GeoClusteringError::UnknownClusterId(cluster_id.clone()))
    }

    fn active_clusters_reaching_item(
        &self,
        target_item_id: &OpaqueId,
        override_item: Option<&GeoEvidenceItem>,
    ) -> Vec<(OpaqueId, VersionId)> {
        let mut result = Vec::new();
        for (cid, cluster) in &self.active_clusters {
            if self.members_reach_item(&cluster.member_item_ids, target_item_id, override_item) {
                result.push((cid.clone(), cluster.version_id.clone()));
            }
        }
        result
    }

    fn members_reach_item(
        &self,
        member_ids: &BTreeSet<OpaqueId>,
        target_item_id: &OpaqueId,
        override_item: Option<&GeoEvidenceItem>,
    ) -> bool {
        if member_ids.contains(target_item_id) {
            return true;
        }
        let mut visited = BTreeSet::new();
        let mut stack: Vec<OpaqueId> = member_ids.iter().cloned().collect();
        while let Some(curr) = stack.pop() {
            if &curr == target_item_id {
                return true;
            }
            if !visited.insert(curr.clone()) {
                continue;
            }
            let item_opt = match override_item {
                Some(ov) if ov.item_id == curr => Some(ov),
                _ => self.items.get(&curr),
            };
            if let Some(item) = item_opt {
                for link in &item.lineage_links {
                    if &link.upstream_item_id == target_item_id {
                        return true;
                    }
                    if link.collapses_independent_source() {
                        stack.push(link.upstream_item_id.clone());
                    }
                }
            }
        }
        false
    }

    /// Combines reassessment reasons across `versions` and rejects with
    /// [`GeoClusteringError::BudgetExceeded`] if the deduplicated total exceeds
    /// `self.limits.max_reassessment_reasons_per_cluster` (never truncating silently).
    fn combine_reassessment_reasons_checked(
        &self,
        versions: &[VersionId],
    ) -> Result<Vec<String>, GeoClusteringError> {
        let mut seen = BTreeSet::new();
        let mut combined = Vec::new();
        for ver in versions {
            if let Some(reasons) = self.reassessment_by_cluster_version.get(ver) {
                for r in reasons {
                    if seen.insert(r.clone()) {
                        combined.push(r.clone());
                    }
                }
            }
        }
        if combined.len() > self.limits.max_reassessment_reasons_per_cluster {
            return Err(GeoClusteringError::BudgetExceeded {
                limit: ClusterLimitKind::ReassessmentReasonsPerCluster,
                actual: combined.len(),
                max: self.limits.max_reassessment_reasons_per_cluster,
            });
        }
        Ok(combined)
    }

    /// Computes the reassessment reasons relevant to a restored cluster on revert:
    /// includes all reasons from `own_version`, plus any reasons added on `later_versions`
    /// that apply to `member_ids` (avoiding cross-cluster leakage of `member <other> revised`
    /// reasons from unrelated sibling members).
    fn relevant_reassessment_reasons_for_members(
        &self,
        own_version: &VersionId,
        later_versions: &[VersionId],
        member_ids: &BTreeSet<OpaqueId>,
        pinned_item_versions: &BTreeMap<OpaqueId, VersionId>,
    ) -> Result<Vec<String>, GeoClusteringError> {
        let mut seen = BTreeSet::new();
        let mut combined = Vec::new();
        if let Some(own_reasons) = self.reassessment_by_cluster_version.get(own_version) {
            for r in own_reasons {
                if seen.insert(r.clone()) {
                    combined.push(r.clone());
                }
            }
        }
        for ver in later_versions {
            if let Some(reasons) = self.reassessment_by_cluster_version.get(ver) {
                for r in reasons {
                    let keep = if let Some(rest) = r.strip_prefix("member ") {
                        let member_token = rest.split_whitespace().next().unwrap_or("");
                        member_ids.iter().any(|m| m.as_str() == member_token)
                    } else {
                        true
                    };
                    if keep && seen.insert(r.clone()) {
                        combined.push(r.clone());
                    }
                }
            }
        }
        for (item_id, pinned_ver) in pinned_item_versions {
            if let Some(current_item) = self.items.get(item_id)
                && &current_item.version_id != pinned_ver
            {
                let note = format!(
                    "member {item_id} pinned at {pinned_ver} has newer ledger version {}",
                    current_item.version_id
                );
                if seen.insert(note.clone()) {
                    combined.push(note);
                }
            }
        }
        if combined.len() > self.limits.max_reassessment_reasons_per_cluster {
            return Err(GeoClusteringError::BudgetExceeded {
                limit: ClusterLimitKind::ReassessmentReasonsPerCluster,
                actual: combined.len(),
                max: self.limits.max_reassessment_reasons_per_cluster,
            });
        }
        Ok(combined)
    }

    /// Merges two active clusters into a new cluster version while retaining prior
    /// snapshots in `archived_cluster_versions` and appending a reversible decision (`GEO-03`, `A04`).
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
    ) -> Result<&ProposedEventCluster, GeoClusteringError> {
        if cluster_a_id == cluster_b_id {
            return Err(GeoClusteringError::DuplicateClusterId(cluster_a_id.clone()));
        }
        if self.decision_history.len() >= self.limits.max_decision_history {
            return Err(GeoClusteringError::BudgetExceeded {
                limit: ClusterLimitKind::DecisionHistory,
                actual: self.decision_history.len() + 1,
                max: self.limits.max_decision_history,
            });
        }
        criteria.validate_against_limits(self.limits)?;
        if self
            .decision_history
            .iter()
            .any(|d| d.decision_id == decision_id)
        {
            return Err(GeoClusteringError::DuplicateDecisionId(decision_id));
        }
        if self.used_versions.contains(&merged_version_id) {
            return Err(GeoClusteringError::DuplicateVersionId(merged_version_id));
        }
        if merged_cluster_id != *cluster_a_id
            && merged_cluster_id != *cluster_b_id
            && self.active_clusters.contains_key(&merged_cluster_id)
        {
            return Err(GeoClusteringError::DuplicateClusterId(merged_cluster_id));
        }
        let rationale = rationale.into();
        validate_text("rationale", &rationale, self.limits.max_text_bytes)?;

        let Some(cluster_a) = self.active_clusters.get(cluster_a_id).cloned() else {
            return Err(GeoClusteringError::UnknownClusterId(cluster_a_id.clone()));
        };
        let Some(cluster_b) = self.active_clusters.get(cluster_b_id).cloned() else {
            return Err(GeoClusteringError::UnknownClusterId(cluster_b_id.clone()));
        };

        let mut union_members = cluster_a.member_item_ids.clone();
        union_members.extend(cluster_b.member_item_ids.iter().cloned());
        let inherited_reasons = self.combine_reassessment_reasons_checked(&[
            cluster_a.version_id.clone(),
            cluster_b.version_id.clone(),
        ])?;

        let merged_snapshot = self.build_cluster_snapshot(
            merged_cluster_id.clone(),
            merged_version_id.clone(),
            vec![cluster_a.version_id.clone(), cluster_b.version_id.clone()],
            union_members,
            criteria,
            inherited_reasons.clone(),
            rationale.clone(),
        )?;

        self.active_clusters.remove(cluster_a_id);
        self.active_clusters.remove(cluster_b_id);
        self.used_versions.insert(merged_version_id.clone());
        if !inherited_reasons.is_empty() {
            self.reassessment_by_cluster_version
                .insert(merged_version_id.clone(), inherited_reasons);
        }
        self.archived_cluster_versions
            .insert(merged_version_id.clone(), merged_snapshot.clone());
        self.active_clusters
            .insert(merged_cluster_id.clone(), merged_snapshot);
        self.decision_history.push(ClusterDecisionRecord {
            decision_id,
            actor,
            kind: ClusterDecisionKind::Merge {
                source_cluster_a: Box::new(cluster_a),
                source_cluster_b: Box::new(cluster_b),
                merged_cluster_id: merged_cluster_id.clone(),
                merged_version_id,
            },
            rationale,
            reverted_by: None,
        });
        self.active_clusters
            .get(&merged_cluster_id)
            .ok_or(GeoClusteringError::UnknownClusterId(merged_cluster_id))
    }

    /// Splits an active cluster into two disjoint, non-empty child clusters while
    /// retaining prior snapshots and appending a reversible decision (`GEO-03`, `A04`).
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
    ) -> Result<(), GeoClusteringError> {
        if self.active_clusters.len() >= self.limits.max_clusters {
            return Err(GeoClusteringError::BudgetExceeded {
                limit: ClusterLimitKind::Clusters,
                actual: self.active_clusters.len() + 1,
                max: self.limits.max_clusters,
            });
        }
        if self.decision_history.len() >= self.limits.max_decision_history {
            return Err(GeoClusteringError::BudgetExceeded {
                limit: ClusterLimitKind::DecisionHistory,
                actual: self.decision_history.len() + 1,
                max: self.limits.max_decision_history,
            });
        }
        criteria.validate_against_limits(self.limits)?;
        if left_cluster_id == right_cluster_id {
            return Err(GeoClusteringError::DuplicateClusterId(left_cluster_id));
        }
        if left_version_id == right_version_id {
            return Err(GeoClusteringError::DuplicateVersionId(left_version_id));
        }
        if self.used_versions.contains(&left_version_id) {
            return Err(GeoClusteringError::DuplicateVersionId(left_version_id));
        }
        if self.used_versions.contains(&right_version_id) {
            return Err(GeoClusteringError::DuplicateVersionId(right_version_id));
        }
        if self
            .decision_history
            .iter()
            .any(|d| d.decision_id == decision_id)
        {
            return Err(GeoClusteringError::DuplicateDecisionId(decision_id));
        }
        let rationale = rationale.into();
        validate_text("rationale", &rationale, self.limits.max_text_bytes)?;

        let Some(original) = self.active_clusters.get(source_cluster_id).cloned() else {
            return Err(GeoClusteringError::UnknownClusterId(
                source_cluster_id.clone(),
            ));
        };
        if left_cluster_id != *source_cluster_id
            && self.active_clusters.contains_key(&left_cluster_id)
        {
            return Err(GeoClusteringError::DuplicateClusterId(left_cluster_id));
        }
        if right_cluster_id != *source_cluster_id
            && self.active_clusters.contains_key(&right_cluster_id)
        {
            return Err(GeoClusteringError::DuplicateClusterId(right_cluster_id));
        }
        if left_members.is_empty() || right_members.is_empty() {
            return Err(GeoClusteringError::InvalidClusterSplitPartition {
                cluster_id: source_cluster_id.clone(),
                reason: "both split partitions must be non-empty",
            });
        }
        if !left_members.is_disjoint(&right_members) {
            return Err(GeoClusteringError::InvalidClusterSplitPartition {
                cluster_id: source_cluster_id.clone(),
                reason: "split partitions must be disjoint",
            });
        }
        let mut combined = left_members.clone();
        combined.extend(right_members.iter().cloned());
        if combined != original.member_item_ids {
            return Err(GeoClusteringError::InvalidClusterSplitPartition {
                cluster_id: source_cluster_id.clone(),
                reason: "split partitions must equal the original cluster members",
            });
        }

        let inherited_reasons =
            self.combine_reassessment_reasons_checked(std::slice::from_ref(&original.version_id))?;

        let left_snapshot = self.build_cluster_snapshot(
            left_cluster_id.clone(),
            left_version_id.clone(),
            vec![original.version_id.clone()],
            left_members,
            criteria.clone(),
            inherited_reasons.clone(),
            rationale.clone(),
        )?;
        let right_snapshot = self.build_cluster_snapshot(
            right_cluster_id.clone(),
            right_version_id.clone(),
            vec![original.version_id.clone()],
            right_members,
            criteria,
            inherited_reasons.clone(),
            rationale.clone(),
        )?;

        self.active_clusters.remove(source_cluster_id);
        self.used_versions.insert(left_version_id.clone());
        self.used_versions.insert(right_version_id.clone());
        if !inherited_reasons.is_empty() {
            self.reassessment_by_cluster_version
                .insert(left_version_id.clone(), inherited_reasons.clone());
            self.reassessment_by_cluster_version
                .insert(right_version_id.clone(), inherited_reasons);
        }
        self.archived_cluster_versions
            .insert(left_version_id.clone(), left_snapshot.clone());
        self.archived_cluster_versions
            .insert(right_version_id.clone(), right_snapshot.clone());
        self.active_clusters
            .insert(left_cluster_id.clone(), left_snapshot);
        self.active_clusters
            .insert(right_cluster_id.clone(), right_snapshot);
        self.decision_history.push(ClusterDecisionRecord {
            decision_id,
            actor,
            kind: ClusterDecisionKind::Split {
                original_cluster: Box::new(original),
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

    /// Reverts a prior merge, split, or reassess decision by restoring the pre-decision
    /// cluster snapshots under new immutable [`VersionId`]s while keeping `decision_history`
    /// and `archived_cluster_versions` append-only (`GEO-03`, `A04`).
    ///
    /// Preserves the pre-decision `pinned_item_versions` and `rationale` from the
    /// archived snapshots, carries forward any reassessment reasons relevant to the
    /// restored cluster's members, and rejects the revert with
    /// [`GeoClusteringError::RevertDestinationClusterInUse`] if a destination
    /// `cluster_id` was reused by another active cluster in the meantime.
    pub fn revert_decision(
        &mut self,
        target_decision_id: &OpaqueId,
        new_revert_decision_id: OpaqueId,
        restored_version_ids: &[VersionId],
        actor: ActorId,
        rationale: impl Into<String>,
    ) -> Result<(), GeoClusteringError> {
        if self.decision_history.len() >= self.limits.max_decision_history {
            return Err(GeoClusteringError::BudgetExceeded {
                limit: ClusterLimitKind::DecisionHistory,
                actual: self.decision_history.len() + 1,
                max: self.limits.max_decision_history,
            });
        }
        if self
            .decision_history
            .iter()
            .any(|d| d.decision_id == new_revert_decision_id)
        {
            return Err(GeoClusteringError::DuplicateDecisionId(
                new_revert_decision_id,
            ));
        }
        let rationale = rationale.into();
        validate_text("rationale", &rationale, self.limits.max_text_bytes)?;

        let mut unique_new_versions = BTreeSet::new();
        for v in restored_version_ids {
            if self.used_versions.contains(v) || !unique_new_versions.insert(v.clone()) {
                return Err(GeoClusteringError::DuplicateVersionId(v.clone()));
            }
        }

        let Some(target_idx) = self
            .decision_history
            .iter()
            .position(|d| &d.decision_id == target_decision_id)
        else {
            return Err(GeoClusteringError::UnknownDecisionId(
                target_decision_id.clone(),
            ));
        };
        if self.decision_history[target_idx].reverted_by.is_some() {
            return Err(GeoClusteringError::DecisionAlreadyReverted(
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
                if self.active_clusters.len() >= self.limits.max_clusters {
                    return Err(GeoClusteringError::BudgetExceeded {
                        limit: ClusterLimitKind::Clusters,
                        actual: self.active_clusters.len() + 1,
                        max: self.limits.max_clusters,
                    });
                }
                let [ver_a, ver_b] = restored_version_ids else {
                    return Err(GeoClusteringError::InvalidRevertVersionCount {
                        decision_id: target_decision_id.clone(),
                        expected: 2,
                        supplied: restored_version_ids.len(),
                    });
                };
                let current = self.active_clusters.get(&merged_cluster_id);
                if current.map(|c| &c.version_id) != Some(&merged_version_id) {
                    return Err(GeoClusteringError::DecisionNoLongerCurrent {
                        decision_id: target_decision_id.clone(),
                        missing_or_modified_cluster: merged_cluster_id,
                    });
                }
                for dest_id in [&source_cluster_a.cluster_id, &source_cluster_b.cluster_id] {
                    if *dest_id != merged_cluster_id && self.active_clusters.contains_key(dest_id) {
                        return Err(GeoClusteringError::RevertDestinationClusterInUse {
                            decision_id: target_decision_id.clone(),
                            conflicting_cluster_id: dest_id.clone(),
                        });
                    }
                }

                let reasons_a = self.relevant_reassessment_reasons_for_members(
                    &source_cluster_a.version_id,
                    std::slice::from_ref(&merged_version_id),
                    &source_cluster_a.member_item_ids,
                    &source_cluster_a.pinned_item_versions,
                )?;
                let reasons_b = self.relevant_reassessment_reasons_for_members(
                    &source_cluster_b.version_id,
                    std::slice::from_ref(&merged_version_id),
                    &source_cluster_b.member_item_ids,
                    &source_cluster_b.pinned_item_versions,
                )?;
                let restored_a = self.restore_archived_snapshot(
                    &source_cluster_a,
                    ver_a.clone(),
                    vec![
                        source_cluster_a.version_id.clone(),
                        merged_version_id.clone(),
                    ],
                    reasons_a.clone(),
                );
                let restored_b = self.restore_archived_snapshot(
                    &source_cluster_b,
                    ver_b.clone(),
                    vec![source_cluster_b.version_id.clone(), merged_version_id],
                    reasons_b.clone(),
                );

                self.active_clusters.remove(&merged_cluster_id);
                self.used_versions.insert(ver_a.clone());
                self.used_versions.insert(ver_b.clone());
                if !reasons_a.is_empty() {
                    self.reassessment_by_cluster_version
                        .insert(ver_a.clone(), reasons_a);
                }
                if !reasons_b.is_empty() {
                    self.reassessment_by_cluster_version
                        .insert(ver_b.clone(), reasons_b);
                }
                self.archived_cluster_versions
                    .insert(ver_a.clone(), restored_a.clone());
                self.archived_cluster_versions
                    .insert(ver_b.clone(), restored_b.clone());
                restored_pairs.push((restored_a.cluster_id.clone(), ver_a.clone()));
                restored_pairs.push((restored_b.cluster_id.clone(), ver_b.clone()));
                self.active_clusters
                    .insert(restored_a.cluster_id.clone(), restored_a);
                self.active_clusters
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
                    return Err(GeoClusteringError::InvalidRevertVersionCount {
                        decision_id: target_decision_id.clone(),
                        expected: 1,
                        supplied: restored_version_ids.len(),
                    });
                };
                if self
                    .active_clusters
                    .get(&left_cluster_id)
                    .map(|c| &c.version_id)
                    != Some(&left_version_id)
                {
                    return Err(GeoClusteringError::DecisionNoLongerCurrent {
                        decision_id: target_decision_id.clone(),
                        missing_or_modified_cluster: left_cluster_id,
                    });
                }
                if self
                    .active_clusters
                    .get(&right_cluster_id)
                    .map(|c| &c.version_id)
                    != Some(&right_version_id)
                {
                    return Err(GeoClusteringError::DecisionNoLongerCurrent {
                        decision_id: target_decision_id.clone(),
                        missing_or_modified_cluster: right_cluster_id,
                    });
                }
                if original_cluster.cluster_id != left_cluster_id
                    && original_cluster.cluster_id != right_cluster_id
                    && self
                        .active_clusters
                        .contains_key(&original_cluster.cluster_id)
                {
                    return Err(GeoClusteringError::RevertDestinationClusterInUse {
                        decision_id: target_decision_id.clone(),
                        conflicting_cluster_id: original_cluster.cluster_id,
                    });
                }

                let reasons_orig = self.relevant_reassessment_reasons_for_members(
                    &original_cluster.version_id,
                    &[left_version_id.clone(), right_version_id.clone()],
                    &original_cluster.member_item_ids,
                    &original_cluster.pinned_item_versions,
                )?;
                let restored_orig = self.restore_archived_snapshot(
                    &original_cluster,
                    ver_orig.clone(),
                    vec![
                        original_cluster.version_id.clone(),
                        left_version_id,
                        right_version_id,
                    ],
                    reasons_orig.clone(),
                );

                self.active_clusters.remove(&left_cluster_id);
                self.active_clusters.remove(&right_cluster_id);
                self.used_versions.insert(ver_orig.clone());
                if !reasons_orig.is_empty() {
                    self.reassessment_by_cluster_version
                        .insert(ver_orig.clone(), reasons_orig);
                }
                self.archived_cluster_versions
                    .insert(ver_orig.clone(), restored_orig.clone());
                restored_pairs.push((restored_orig.cluster_id.clone(), ver_orig.clone()));
                self.active_clusters
                    .insert(restored_orig.cluster_id.clone(), restored_orig);
            }
            ClusterDecisionKind::Reassess {
                prior_cluster,
                reassessed_version_id,
                acknowledged_reasons,
            } => {
                let [ver_restored] = restored_version_ids else {
                    return Err(GeoClusteringError::InvalidRevertVersionCount {
                        decision_id: target_decision_id.clone(),
                        expected: 1,
                        supplied: restored_version_ids.len(),
                    });
                };
                if self
                    .active_clusters
                    .get(&prior_cluster.cluster_id)
                    .map(|c| &c.version_id)
                    != Some(&reassessed_version_id)
                {
                    return Err(GeoClusteringError::DecisionNoLongerCurrent {
                        decision_id: target_decision_id.clone(),
                        missing_or_modified_cluster: prior_cluster.cluster_id,
                    });
                }
                let mut combined = acknowledged_reasons;
                let later = self.relevant_reassessment_reasons_for_members(
                    &prior_cluster.version_id,
                    std::slice::from_ref(&reassessed_version_id),
                    &prior_cluster.member_item_ids,
                    &prior_cluster.pinned_item_versions,
                )?;
                for r in later {
                    if !combined.contains(&r) {
                        combined.push(r);
                    }
                }
                if combined.len() > self.limits.max_reassessment_reasons_per_cluster {
                    return Err(GeoClusteringError::BudgetExceeded {
                        limit: ClusterLimitKind::ReassessmentReasonsPerCluster,
                        actual: combined.len(),
                        max: self.limits.max_reassessment_reasons_per_cluster,
                    });
                }
                let restored = self.restore_archived_snapshot(
                    &prior_cluster,
                    ver_restored.clone(),
                    vec![prior_cluster.version_id.clone(), reassessed_version_id],
                    combined.clone(),
                );
                self.used_versions.insert(ver_restored.clone());
                if !combined.is_empty() {
                    self.reassessment_by_cluster_version
                        .insert(ver_restored.clone(), combined);
                }
                self.archived_cluster_versions
                    .insert(ver_restored.clone(), restored.clone());
                restored_pairs.push((restored.cluster_id.clone(), ver_restored.clone()));
                self.active_clusters
                    .insert(restored.cluster_id.clone(), restored);
            }
            ClusterDecisionKind::Revert { .. } => {
                return Err(GeoClusteringError::CannotRevertRevertDecision(
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

    fn restore_archived_snapshot(
        &self,
        archived: &ProposedEventCluster,
        new_version_id: VersionId,
        predecessor_version_ids: Vec<VersionId>,
        reassessment_reasons: Vec<String>,
    ) -> ProposedEventCluster {
        let _ = self;
        ProposedEventCluster {
            cluster_id: archived.cluster_id.clone(),
            version_id: new_version_id,
            predecessor_version_ids,
            member_item_ids: archived.member_item_ids.clone(),
            pinned_item_versions: archived.pinned_item_versions.clone(),
            criteria: archived.criteria.clone(),
            conflicts: archived.conflicts.clone(),
            counts: archived.counts,
            needs_reassessment_at_issue: !reassessment_reasons.is_empty(),
            reassessment_reasons_at_issue: reassessment_reasons,
            rationale: archived.rationale.clone(),
        }
    }

    /// Computes ledger-wide distinct counts of articles, witness accounts, sensor
    /// observations, independent sources (total, supportive, contradictory, mixed),
    /// proposed event clusters, and simulated items (`GEO-03`, `A04`).
    #[must_use]
    pub fn ledger_wide_counts(&self) -> DistinctEvidenceCounts {
        let all_ids: BTreeSet<OpaqueId> = self.items.keys().cloned().collect();
        let (mut counts, _) = self.compute_counts_stances_and_cycles(&all_ids);
        counts.proposed_event_count = self.active_clusters.len();
        counts
    }

    #[allow(clippy::too_many_arguments)]
    fn build_cluster_snapshot(
        &self,
        cluster_id: OpaqueId,
        version_id: VersionId,
        predecessor_version_ids: Vec<VersionId>,
        member_item_ids: BTreeSet<OpaqueId>,
        criteria: ClusterMatchCriteria,
        reassessment_reasons: Vec<String>,
        rationale: String,
    ) -> Result<ProposedEventCluster, GeoClusteringError> {
        let mut members = Vec::with_capacity(member_item_ids.len());
        let mut pinned_item_versions = BTreeMap::new();
        for id in &member_item_ids {
            let Some(item) = self.items.get(id) else {
                return Err(GeoClusteringError::UnknownEvidenceItem(id.clone()));
            };
            pinned_item_versions.insert(id.clone(), item.version_id.clone());
            members.push(item);
        }

        let mut conflicts = Vec::new();
        for (idx_a, item_a) in members.iter().enumerate() {
            if let LocationQualificationKind::ApproximateGazetteerCentroid {
                radius_uncertainty_m,
                ..
            } = item_a.location_qualification.kind()
            {
                conflicts.push(ClusterConflict::ApproximateLocationUsed {
                    item_id: item_a.item_id.clone(),
                    radius_uncertainty_m: Some(*radius_uncertainty_m),
                });
            } else if matches!(
                item_a.location_qualification.kind(),
                LocationQualificationKind::BoundedRegionDescription { .. }
            ) {
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
                        .map(|q| q.earliest_source_id().clone()),
                });
            }
            if item_a.kind.is_simulated() {
                conflicts.push(ClusterConflict::SimulatedEvidencePresent {
                    item_id: item_a.item_id.clone(),
                });
            }
            for req_key in &criteria.required_property_keys {
                if !item_a.reported_properties.contains_key(req_key) {
                    conflicts.push(ClusterConflict::MissingRequiredProperty {
                        item_id: item_a.item_id.clone(),
                        property_key: req_key.clone(),
                    });
                }
            }

            for item_b in members.iter().skip(idx_a + 1) {
                if let Some(&dist_m) = self
                    .pairwise_distances_m
                    .get(&canonical_pair(&item_a.item_id, &item_b.item_id))
                {
                    if dist_m > criteria.max_proximity_m {
                        conflicts.push(ClusterConflict::SpatialSeparationExceedsThreshold {
                            item_a: item_a.item_id.clone(),
                            item_b: item_b.item_id.clone(),
                            distance_m: dist_m,
                            threshold_m: criteria.max_proximity_m,
                        });
                    }
                } else {
                    conflicts.push(ClusterConflict::SpatialSeparationNotEvaluated {
                        item_a: item_a.item_id.clone(),
                        item_b: item_b.item_id.clone(),
                    });
                }

                match item_a
                    .reported_time
                    .compare_temporal_separation(&item_b.reported_time)
                {
                    TemporalComparisonOutcome::ExactSubDayGapMs(gap_ms) => {
                        if gap_ms > criteria.max_time_separation_ms {
                            conflicts.push(ClusterConflict::TemporalSeparationExceedsThreshold {
                                item_a: item_a.item_id.clone(),
                                item_b: item_b.item_id.clone(),
                                gap_ms,
                                threshold_ms: criteria.max_time_separation_ms,
                            });
                        }
                    }
                    TemporalComparisonOutcome::CalendarDateSeparationDays(day_gap) => {
                        if day_gap > criteria.max_calendar_day_separation {
                            conflicts.push(
                                ClusterConflict::CalendarDateSeparationExceedsThreshold {
                                    item_a: item_a.item_id.clone(),
                                    item_b: item_b.item_id.clone(),
                                    day_gap,
                                    threshold_days: criteria.max_calendar_day_separation,
                                },
                            );
                        }
                    }
                    TemporalComparisonOutcome::CalendarDateOnlyCannotResolveSubDayGap {
                        minimum_whole_day_separation,
                    } => {
                        conflicts.push(ClusterConflict::CalendarDateSubDayGapUnresolvable {
                            item_a: item_a.item_id.clone(),
                            item_b: item_b.item_id.clone(),
                            minimum_whole_day_separation,
                        });
                        if minimum_whole_day_separation > criteria.max_calendar_day_separation {
                            conflicts.push(
                                ClusterConflict::CalendarDateSeparationExceedsThreshold {
                                    item_a: item_a.item_id.clone(),
                                    item_b: item_b.item_id.clone(),
                                    day_gap: minimum_whole_day_separation,
                                    threshold_days: criteria.max_calendar_day_separation,
                                },
                            );
                        }
                    }
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

        let (counts, lineage_conflicts) = self.compute_counts_stances_and_cycles(&member_item_ids);
        conflicts.extend(lineage_conflicts);

        Ok(ProposedEventCluster {
            cluster_id,
            version_id,
            predecessor_version_ids,
            member_item_ids,
            pinned_item_versions,
            criteria,
            conflicts,
            counts,
            needs_reassessment_at_issue: !reassessment_reasons.is_empty(),
            reassessment_reasons_at_issue: reassessment_reasons,
            rationale,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn traverse_collapsing_lineage(
        &self,
        curr: &OpaqueId,
        active_path: &mut BTreeSet<OpaqueId>,
        expanded: &mut BTreeSet<OpaqueId>,
        reached_roots: &mut BTreeSet<OpaqueId>,
        traversed_sources: &mut BTreeSet<OpaqueId>,
        hit_cycle: &mut bool,
        hit_simulated: &mut bool,
    ) {
        if active_path.contains(curr) {
            *hit_cycle = true;
            return;
        }
        if expanded.contains(curr) {
            return;
        }
        let Some(item) = self.items.get(curr) else {
            reached_roots.insert(curr.clone());
            return;
        };
        if item.kind.is_simulated() {
            *hit_simulated = true;
            return;
        }
        traversed_sources.insert(item.source_id.clone());
        let collapsing_upstreams: Vec<OpaqueId> = item
            .lineage_links
            .iter()
            .filter(|l| l.collapses_independent_source())
            .map(|l| l.upstream_item_id.clone())
            .collect();
        if collapsing_upstreams.is_empty() {
            reached_roots.insert(curr.clone());
            expanded.insert(curr.clone());
            return;
        }
        active_path.insert(curr.clone());
        for up in collapsing_upstreams {
            if self.items.contains_key(&up) {
                self.traverse_collapsing_lineage(
                    &up,
                    active_path,
                    expanded,
                    reached_roots,
                    traversed_sources,
                    hit_cycle,
                    hit_simulated,
                );
            } else {
                reached_roots.insert(up);
            }
        }
        active_path.remove(curr);
        expanded.insert(curr.clone());
    }

    fn compute_counts_stances_and_cycles(
        &self,
        member_ids: &BTreeSet<OpaqueId>,
    ) -> (DistinctEvidenceCounts, Vec<ClusterConflict>) {
        let mut article_count = 0usize;
        let mut account_count = 0usize;
        let mut sensor_observation_count = 0usize;
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
                    sensor_observation_count += 1;
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

        // Resolve upstream roots and collect all traversed `source_id`s across
        // collapsing lineage links (`Republishes`, `Quotes`, `Supersedes`), tracking
        // the current active path so DAG diamonds into simulated nodes are distinguished
        // from true cycles.
        let mut cycle_members = BTreeSet::new();
        let mut simulated_lineage_only = Vec::new();
        let mut anchored_items = Vec::new();
        let mut root_set_for: BTreeMap<OpaqueId, BTreeSet<OpaqueId>> = BTreeMap::new();
        let mut source_set_for: BTreeMap<OpaqueId, BTreeSet<OpaqueId>> = BTreeMap::new();

        for id in &non_simulated_ids {
            let mut active_path = BTreeSet::new();
            let mut expanded = BTreeSet::new();
            let mut reached_roots = BTreeSet::new();
            let mut traversed_sources = BTreeSet::new();
            let mut hit_cycle = false;
            let mut hit_simulated = false;

            self.traverse_collapsing_lineage(
                id,
                &mut active_path,
                &mut expanded,
                &mut reached_roots,
                &mut traversed_sources,
                &mut hit_cycle,
                &mut hit_simulated,
            );

            if reached_roots.is_empty() {
                if hit_cycle {
                    cycle_members.insert(id.clone());
                } else if hit_simulated {
                    simulated_lineage_only.push(id.clone());
                }
            } else {
                anchored_items.push(id.clone());
                root_set_for.insert(id.clone(), reached_roots);
                source_set_for.insert(id.clone(), traversed_sources);
            }
        }

        // Union-find over anchored items sharing:
        // 1) any common resolved upstream lineage root,
        // 2) any common `source_id` along their collapsing lineage chain, or
        // 3) any identical non-empty `content_hash`.
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
                let shares_lineage_root = match (root_set_for.get(id_i), root_set_for.get(id_j)) {
                    (Some(ri), Some(rj)) => !ri.is_disjoint(rj),
                    _ => false,
                };
                let shares_source = match (source_set_for.get(id_i), source_set_for.get(id_j)) {
                    (Some(si), Some(sj)) => !si.is_disjoint(sj),
                    _ => false,
                };
                let shares_hash = match (self.items.get(id_i), self.items.get(id_j)) {
                    (Some(item_i), Some(item_j)) => {
                        match (&item_i.content_hash, &item_j.content_hash) {
                            (Some(h1), Some(h2)) => h1 == h2,
                            _ => false,
                        }
                    }
                    _ => false,
                };

                if shares_lineage_root || shares_source || shares_hash {
                    let ri = find(&mut parent, i);
                    let rj = find(&mut parent, j);
                    if ri != rj {
                        parent[rj] = ri;
                    }
                }
            }
        }

        let mut classes: BTreeMap<usize, Vec<OpaqueId>> = BTreeMap::new();
        for (i, item_id) in anchored_items.iter().enumerate() {
            let root = find(&mut parent, i);
            classes.entry(root).or_default().push(item_id.clone());
        }

        let mut supportive_independent_source_count = 0usize;
        let mut contradictory_independent_source_count = 0usize;
        let mut mixed_stance_independent_source_count = 0usize;
        let mut lineage_conflicts = Vec::new();

        for class_members in classes.values() {
            let mut member_assessments = BTreeMap::new();
            let mut all_supported = true;
            let mut all_contradicted = true;
            let mut has_disputed = false;

            for mid in class_members {
                let Some(item) = self.items.get(mid) else {
                    continue;
                };
                let st = item.claim_assessment;
                member_assessments.insert(mid.clone(), st);
                if st != ClaimAssessmentState::Supported {
                    all_supported = false;
                }
                if st != ClaimAssessmentState::Contradicted {
                    all_contradicted = false;
                }
                if st == ClaimAssessmentState::Disputed {
                    has_disputed = true;
                }
            }

            let distinct_states: usize = member_assessments
                .values()
                .map(|s| match s {
                    ClaimAssessmentState::Supported => 0u8,
                    ClaimAssessmentState::Contradicted => 1u8,
                    ClaimAssessmentState::UnsupportedByAvailableEvidence => 2u8,
                    ClaimAssessmentState::MateriallyMisleading => 3u8,
                    ClaimAssessmentState::Disputed => 4u8,
                    ClaimAssessmentState::Unresolved => 5u8,
                })
                .collect::<BTreeSet<u8>>()
                .len();

            if all_supported {
                supportive_independent_source_count += 1;
            } else if all_contradicted {
                contradictory_independent_source_count += 1;
            } else if distinct_states > 1 || has_disputed {
                mixed_stance_independent_source_count += 1;
                if let Some(first_id) = class_members.first() {
                    lineage_conflicts.push(ClusterConflict::MixedStanceSourceClass {
                        representative_item_id: first_id.clone(),
                        member_assessments,
                    });
                }
            }
        }

        for item_id in simulated_lineage_only {
            lineage_conflicts
                .push(ClusterConflict::LineageAnchoredOnlyToSimulatedEvidence { item_id });
        }
        if !cycle_members.is_empty() {
            lineage_conflicts.push(ClusterConflict::UnanchoredLineageCycle {
                cycle_members: cycle_members.into_iter().collect(),
            });
        }

        (
            DistinctEvidenceCounts {
                article_count,
                account_count,
                sensor_observation_count,
                independent_source_count: classes.len(),
                supportive_independent_source_count,
                contradictory_independent_source_count,
                mixed_stance_independent_source_count,
                proposed_event_count: 1,
                simulated_item_count,
            },
            lineage_conflicts,
        )
    }
}
