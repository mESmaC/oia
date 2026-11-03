//! Airspace and maritime navigation notice context types (`INTEL-03`).

use crate::geo_context::provisional::{
    Altitude, GeoBoundingBoxWgs84, GeoPointWgs84, GeoPosition, TimeWindow,
};
use crate::geo_context::{GeoContextError, SpatioTemporalEnvelope};
use oia_domain::OpaqueId;

/// Airspace or maritime notice classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    AirspaceRestriction,
    RocketLaunchCorridor,
    MaritimeHazardWarning,
    MilitaryExercise,
    NavigationHazard,
}

/// Horizontal boundary representation for notices.
#[derive(Debug, Clone, PartialEq)]
pub enum NoticeSpatialBoundary {
    Circle {
        center: GeoPointWgs84,
        radius_m: f64,
    },
    BoundingBox(GeoBoundingBoxWgs84),
}

impl NoticeSpatialBoundary {
    #[must_use]
    pub fn contains(&self, point: GeoPointWgs84) -> bool {
        match self {
            Self::Circle { center, radius_m } => center.great_circle_distance_m(point) <= *radius_m,
            Self::BoundingBox(bb) => bb.contains_point(point),
        }
    }

    #[must_use]
    pub fn intersects_box(&self, target_box: &GeoBoundingBoxWgs84) -> bool {
        match self {
            Self::BoundingBox(bb) => bb.overlaps_box(target_box),
            Self::Circle { center, radius_m } => {
                let nearest = target_box.nearest_point_to(*center);
                center.great_circle_distance_m(nearest) <= *radius_m
            }
        }
    }
}

/// Classification of notice overlap with a query envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeEnvelopeOverlap {
    /// Both 2D horizontal/temporal and vertical bounds are confirmed to overlap.
    DefiniteOverlap,
    /// 2D horizontal or temporal bounds do not overlap, or vertical limits definitely do not overlap.
    NoOverlap,
    /// 2D horizontal and temporal bounds overlap, but vertical limits use mixed or incompatible datums.
    VerticalOverlapUnknown,
}

/// Classification of notice intersection with a 3D position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticePositionIntersection {
    /// Position falls within horizontal, temporal, and vertical notice bounds.
    Intersects,
    /// Position is outside horizontal, temporal, or confirmed vertical notice bounds.
    DoesNotIntersect,
    /// Position matches horizontal and temporal bounds, but vertical datums are incompatible.
    VerticalOverlapUnknown,
}

/// Airspace or maritime navigation notice (e.g. NOTAM, NAVWARN, launch restriction).
#[derive(Debug, Clone, PartialEq)]
pub struct NavigationNotice {
    pub notice_id: OpaqueId,
    pub issuing_authority: String,
    pub notice_kind: NoticeKind,
    pub issuance_instant_utc_ms: i64,
    pub retrieval_instant_utc_ms: i64,
    pub effective_window: TimeWindow,
    pub spatial_boundary: NoticeSpatialBoundary,
    pub lower_altitude: Option<Altitude>,
    pub upper_altitude: Option<Altitude>,
    pub summary: String,
    pub raw_content_sha256: [u8; 32],
}

impl NavigationNotice {
    /// Validates structural consistency and temporal integrity.
    pub fn validate(&self) -> Result<(), GeoContextError> {
        match &self.spatial_boundary {
            NoticeSpatialBoundary::Circle { center, radius_m } => {
                center.validate()?;
                if !radius_m.is_finite() || *radius_m < 0.0 {
                    return Err(GeoContextError::InvalidLimitValue {
                        field: "NavigationNotice.radius_m",
                        reason: "radius must be finite and >= 0.0",
                    });
                }
            }
            NoticeSpatialBoundary::BoundingBox(bb) => {
                bb.validate()?;
            }
        }
        self.effective_window.validate()?;
        if self.retrieval_instant_utc_ms < self.issuance_instant_utc_ms {
            return Err(GeoContextError::InvalidTemporalOrdering(
                "notice retrieval_instant_utc_ms must be >= issuance_instant_utc_ms",
            ));
        }
        // If both lower and upper altitudes are specified with the same datum, lower must be <= upper
        if let (Some(lower), Some(upper)) = (&self.lower_altitude, &self.upper_altitude)
            && lower.reference() == upper.reference()
            && lower.meters() > upper.meters()
        {
            return Err(GeoContextError::InvalidAltitudeBand {
                min_meters: lower.meters(),
                max_meters: upper.meters(),
            });
        }
        Ok(())
    }

    #[must_use]
    pub fn is_active_at(&self, instant_utc_ms: i64) -> bool {
        self.effective_window.contains(instant_utc_ms)
    }

    /// Verifies if a 3D position at timestamp falls inside this notice volume.
    ///
    /// Distinguishes confirmed intersection, non-intersection, and vertical overlap unknown
    /// when datums cannot be compared without external geophysical models.
    #[must_use]
    pub fn intersects_position(
        &self,
        pos: &GeoPosition,
        instant_utc_ms: i64,
    ) -> NoticePositionIntersection {
        if !self.is_active_at(instant_utc_ms) {
            return NoticePositionIntersection::DoesNotIntersect;
        }
        if !self.spatial_boundary.contains(pos.point()) {
            return NoticePositionIntersection::DoesNotIntersect;
        }

        if self.lower_altitude.is_some() || self.upper_altitude.is_some() {
            match pos.altitude() {
                Some(pos_alt) => {
                    if let Some(ref lower) = self.lower_altitude {
                        if lower.reference() == pos_alt.reference() {
                            if pos_alt.meters() < lower.meters() {
                                return NoticePositionIntersection::DoesNotIntersect;
                            }
                        } else {
                            return NoticePositionIntersection::VerticalOverlapUnknown;
                        }
                    }
                    if let Some(ref upper) = self.upper_altitude {
                        if upper.reference() == pos_alt.reference() {
                            if pos_alt.meters() > upper.meters() {
                                return NoticePositionIntersection::DoesNotIntersect;
                            }
                        } else {
                            return NoticePositionIntersection::VerticalOverlapUnknown;
                        }
                    }
                    NoticePositionIntersection::Intersects
                }
                None => NoticePositionIntersection::VerticalOverlapUnknown,
            }
        } else {
            NoticePositionIntersection::Intersects
        }
    }

    /// Verifies if this notice overlaps with the 4D query envelope.
    ///
    /// Correctly identifies definite overlap, no overlap, or unknown vertical overlap
    /// when vertical limits use mixed datums (e.g., SFC AGL to FL180 barometric).
    #[must_use]
    pub fn overlaps_envelope(&self, envelope: &SpatioTemporalEnvelope) -> NoticeEnvelopeOverlap {
        if !self.effective_window.overlaps(&envelope.time_window) {
            return NoticeEnvelopeOverlap::NoOverlap;
        }
        if !self
            .spatial_boundary
            .intersects_box(&envelope.spatial_bounds)
        {
            return NoticeEnvelopeOverlap::NoOverlap;
        }

        if let Some(ref env_band) = envelope.altitude_band {
            match (&self.lower_altitude, &self.upper_altitude) {
                (Some(lower), Some(upper)) => {
                    if lower.reference() == upper.reference()
                        && lower.reference() == env_band.reference()
                    {
                        if lower.meters() <= env_band.max_meters()
                            && upper.meters() >= env_band.min_meters()
                        {
                            NoticeEnvelopeOverlap::DefiniteOverlap
                        } else {
                            NoticeEnvelopeOverlap::NoOverlap
                        }
                    } else {
                        // Notice limits mix datums (e.g. SFC AGL to FL180 barometric),
                        // or notice datum differs from envelope datum.
                        NoticeEnvelopeOverlap::VerticalOverlapUnknown
                    }
                }
                (Some(lower), None) => {
                    if lower.reference() == env_band.reference() {
                        if lower.meters() <= env_band.max_meters() {
                            NoticeEnvelopeOverlap::DefiniteOverlap
                        } else {
                            NoticeEnvelopeOverlap::NoOverlap
                        }
                    } else {
                        NoticeEnvelopeOverlap::VerticalOverlapUnknown
                    }
                }
                (None, Some(upper)) => {
                    if upper.reference() == env_band.reference() {
                        if upper.meters() >= env_band.min_meters() {
                            NoticeEnvelopeOverlap::DefiniteOverlap
                        } else {
                            NoticeEnvelopeOverlap::NoOverlap
                        }
                    } else {
                        NoticeEnvelopeOverlap::VerticalOverlapUnknown
                    }
                }
                (None, None) => NoticeEnvelopeOverlap::DefiniteOverlap,
            }
        } else {
            NoticeEnvelopeOverlap::DefiniteOverlap
        }
    }
}

/// Summary of navigation notices evaluated for the query envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeReconstructionSummary {
    pub active_matching_notices_count: usize,
    pub vertical_overlap_unknown_count: usize,
}
