//! Space observation, satellite imagery catalog, scientific provenance, and mission stream kernel (`SPACE-01`–`SPACE-03`, `GEO-02`, `GEO-04`, `INTEL-03`, `A05`).
//!
//! Provides provider-independent cataloguing, multi-criteria filtering, scientific product vs.
//! preview provenance tracking, public mission/station stream classification, exact mosaic coverage
//! boundary accounting, and geometry-aware baseline comparison without fabricated detections.
//!
//! ### Core Contracts and Integrity Invariants (§2, §3, §6; `SPACE-01`–`SPACE-03`, `A05`)
//! 1. **Scientific vs. Preview Provenance (`SPACE-02`, `A05`):** Raw scientific products (FITS,
//!    calibrated radiance GeoTIFFs) remain linked to derived previews with explicit calibration
//!    levels, exposure durations, stacking histories, and color mappings. Outreach renderings
//!    and false-color composites are strictly labeled and **NEVER** presented as natural
//!    observations or physical truth measurements.
//! 2. **Continuous Coverage Not Implied (`GEO-04`, `A04`):** Mosaic queries over terrestrial
//!    envelopes compute exact clipped polygon/interval union coverage, supporting antimeridian
//!    dateline crossings on both envelopes and footprints. Seamless continuous coverage is never
//!    assumed or claimed when coverage is partial.
//! 3. **Stream Classification Rigor (`SPACE-03`, `A05`):** Public space mission feeds are classified
//!    into distinct, immutable categories: `LiveCamera`, `DelayedTransmission`, `ReplayArchive`,
//!    `CommentaryBroadcast`, and `SimulationOrRecreation`. Simulated animations or replays never
//!    masquerade as live camera telemetry. Bookmarks pin feed identity and calculate event time from
//!    stream timing metadata using checked arithmetic.
//! 4. **Geometry-Aware Baseline Comparison (`INTEL-03`):** Baseline comparisons revalidate input
//!    capture geometries at the analysis boundary, retain capture geometry, and flag confounding
//!    limitations. Without an actual pixel detector or pinned evidence, change detection is reported
//!    as `RasterDetectorNotPerformed` and never fabricates candidates or synthetic anomaly scores.
//! 5. **Multi-Source Globe Composite Planning & Unredacted Source Selection (`GEO-02`, `GEO-04`, `SPACE-01`, `SPACE-02`, `MEDIA-02`, `MEDIA-03`, `FIELD-03`, `A04`, `A05`):**
//!    Plans multi-source terrestrial composite cell assignments across orbital Earth-observation satellites,
//!    space-station crew frames, national aerial orthophotos, calibrated UAV orthomosaics, and 3D-posed
//!    ground imagery. Enforces verifiable optical resolution (`max(native_gsd_m, served_grid_spacing_m)`),
//!    selects unredacted/cloud-free alternate passes over cells affected by localized statutory blur masks
//!    or cloud/shadow obscurations, rejects outreach/simulated/predictive inpainting and uncalibrated or
//!    unrectified oblique placements, isolates SAR radar backscatter fallbacks, and reports explicit
//!    multi-temporal seam boundaries without fabricating single-instant continuous coverage.
//!    **Explicit Scope Exclusions:** (a) No deblurring, super-resolution, or inpainting of redacted or
//!    low-resolution patches is ever performed; (b) no restricted, credentialed, or higher-tier access
//!    requests are modeled or issued.

use std::error::Error;
use std::fmt;

pub use crate::multilingual::StopReason;
use oia_time_geo::{GeoBoundingBoxWgs84, GeoPointWgs84, TimeWindow};

/// Method identifier for space imagery catalog and observation analysis.
pub const METHOD_ID: &str = "oia-space-imagery-catalog-v1";

/// Method identifier for multi-source terrestrial globe composite planning.
pub const COMPOSITE_PLAN_METHOD_ID: &str = "oia-space-globe-composite-plan-v1";

/// Maximum allowed string identifier length to bound memory allocations.
pub const MAX_STRING_LEN: usize = 256;

/// Cooperative cancellation token hook.
pub type CancelToken<'a> = &'a mut (dyn FnMut() -> Result<(), StopReason> + 'a);

/// Operational bounding limits for space imagery catalog operations (`AGENTS.md` §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceCatalogLimits {
    pub max_observations: usize,
    pub max_query_results: usize,
    pub max_bookmarks_per_feed: usize,
    pub max_stack_inputs: usize,
    pub max_change_candidates: usize,
    pub max_footprints_per_mosaic: usize,
    pub max_previews_per_observation: usize,
    pub max_composite_candidates: usize,
    pub max_composite_grid_cells: usize,
    pub max_defect_regions_per_patch: usize,
    pub max_composite_rejected_receipts: usize,
}

impl Default for SpaceCatalogLimits {
    fn default() -> Self {
        Self {
            max_observations: 10_000,
            max_query_results: 500,
            max_bookmarks_per_feed: 100,
            max_stack_inputs: 64,
            max_change_candidates: 100,
            max_footprints_per_mosaic: 256,
            max_previews_per_observation: 32,
            max_composite_candidates: 512,
            max_composite_grid_cells: 4_096,
            max_defect_regions_per_patch: 64,
            max_composite_rejected_receipts: 16_384,
        }
    }
}

/// Typed domain and validation errors for space imagery operations.
#[derive(Debug, Clone, PartialEq)]
pub enum SpaceImageryError {
    EmptyIdentifier {
        field: &'static str,
    },
    StringTooLong {
        field: &'static str,
        max: usize,
        actual: usize,
    },
    InvalidWavelengthRange {
        min_nm: f64,
        max_nm: f64,
        reason: &'static str,
    },
    InvalidExposureDuration {
        seconds: f64,
        reason: &'static str,
    },
    InvalidResolution {
        value: f64,
        reason: &'static str,
    },
    InvalidCoordinates {
        reason: &'static str,
    },
    InvalidTimeWindow {
        reason: &'static str,
    },
    TimestampOverflow {
        operation: &'static str,
        reason: &'static str,
    },
    InvalidFeedAssociation {
        bookmark_feed_id: String,
        host_feed_id: String,
    },
    BudgetExceeded {
        resource: &'static str,
        limit: usize,
    },
    DuplicateObservationId {
        id: String,
    },
    DuplicateFeedId {
        id: String,
    },
    DuplicateCandidatePatchId {
        id: String,
    },
    DuplicateDefectId {
        patch_id: String,
        defect_id: String,
    },
    ObservationNotFound {
        id: String,
    },
    IncompatibleBaselineComparison {
        reason: &'static str,
    },
    InvalidCompositePolicy {
        reason: &'static str,
    },
    InvalidPatchIntegrity {
        reason: &'static str,
    },
    InvalidRegistration {
        reason: &'static str,
    },
    InvalidRadiometricModality {
        reason: &'static str,
    },
    Stopped(StopReason),
}

impl fmt::Display for SpaceImageryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyIdentifier { field } => write!(f, "field '{field}' cannot be empty"),
            Self::StringTooLong { field, max, actual } => {
                write!(f, "field '{field}' length {actual} exceeds limit {max}")
            }
            Self::InvalidWavelengthRange {
                min_nm,
                max_nm,
                reason,
            } => {
                write!(
                    f,
                    "invalid wavelength range [{min_nm}, {max_nm}] nm: {reason}"
                )
            }
            Self::InvalidExposureDuration { seconds, reason } => {
                write!(f, "invalid exposure duration {seconds} s: {reason}")
            }
            Self::InvalidResolution { value, reason } => {
                write!(f, "invalid spatial resolution {value}: {reason}")
            }
            Self::InvalidCoordinates { reason } => {
                write!(f, "invalid spatial coordinates: {reason}")
            }
            Self::InvalidTimeWindow { reason } => write!(f, "invalid time window: {reason}"),
            Self::TimestampOverflow { operation, reason } => {
                write!(f, "timestamp overflow during '{operation}': {reason}")
            }
            Self::InvalidFeedAssociation {
                bookmark_feed_id,
                host_feed_id,
            } => {
                write!(
                    f,
                    "bookmark belongs to feed '{bookmark_feed_id}', not host feed '{host_feed_id}'"
                )
            }
            Self::BudgetExceeded { resource, limit } => {
                write!(
                    f,
                    "resource limit exceeded for '{resource}': limit is {limit}"
                )
            }
            Self::DuplicateObservationId { id } => {
                write!(f, "duplicate space observation ID: '{id}'")
            }
            Self::DuplicateFeedId { id } => {
                write!(f, "duplicate space mission feed ID: '{id}'")
            }
            Self::DuplicateCandidatePatchId { id } => {
                write!(f, "duplicate composite candidate patch ID: '{id}'")
            }
            Self::DuplicateDefectId {
                patch_id,
                defect_id,
            } => {
                write!(
                    f,
                    "duplicate localized defect ID '{defect_id}' on patch '{patch_id}'"
                )
            }
            Self::ObservationNotFound { id } => {
                write!(f, "space observation ID not found: '{id}'")
            }
            Self::IncompatibleBaselineComparison { reason } => {
                write!(f, "incompatible baseline comparison: {reason}")
            }
            Self::InvalidCompositePolicy { reason } => {
                write!(f, "invalid composite stitch policy: {reason}")
            }
            Self::InvalidPatchIntegrity { reason } => {
                write!(f, "invalid candidate patch integrity state: {reason}")
            }
            Self::InvalidRegistration { reason } => {
                write!(f, "invalid cross-view spatial registration: {reason}")
            }
            Self::InvalidRadiometricModality { reason } => {
                write!(f, "invalid radiometric modality: {reason}")
            }
            Self::Stopped(reason) => write!(f, "operation stopped: {reason:?}"),
        }
    }
}

impl Error for SpaceImageryError {}

/// Broad electromagnetic spectrum regimes for space observations (`SPACE-01`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpectralRegime {
    Radio,
    Microwave,
    FarInfrared,
    MidInfrared,
    NearInfrared,
    OpticalVisible,
    Ultraviolet,
    ExtremeUltraviolet,
    XRay,
    GammaRay,
    Multispectral,
    Hyperspectral,
}

impl fmt::Display for SpectralRegime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Radio => write!(f, "Radio"),
            Self::Microwave => write!(f, "Microwave"),
            Self::FarInfrared => write!(f, "Far-Infrared"),
            Self::MidInfrared => write!(f, "Mid-Infrared"),
            Self::NearInfrared => write!(f, "Near-Infrared"),
            Self::OpticalVisible => write!(f, "Optical / Visible"),
            Self::Ultraviolet => write!(f, "Ultraviolet"),
            Self::ExtremeUltraviolet => write!(f, "Extreme Ultraviolet"),
            Self::XRay => write!(f, "X-Ray"),
            Self::GammaRay => write!(f, "Gamma-Ray"),
            Self::Multispectral => write!(f, "Multispectral"),
            Self::Hyperspectral => write!(f, "Hyperspectral"),
        }
    }
}

/// Explicit wavelength range in nanometres (`SPACE-01`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WavelengthRangeNm {
    pub min_nm: f64,
    pub max_nm: f64,
}

impl WavelengthRangeNm {
    pub fn new(min_nm: f64, max_nm: f64) -> Result<Self, SpaceImageryError> {
        let range = Self { min_nm, max_nm };
        range.validate()?;
        Ok(range)
    }

    pub fn validate(&self) -> Result<(), SpaceImageryError> {
        if !self.min_nm.is_finite() || self.min_nm <= 0.0 {
            return Err(SpaceImageryError::InvalidWavelengthRange {
                min_nm: self.min_nm,
                max_nm: self.max_nm,
                reason: "minimum wavelength must be finite and positive",
            });
        }
        if !self.max_nm.is_finite() || self.max_nm < self.min_nm {
            return Err(SpaceImageryError::InvalidWavelengthRange {
                min_nm: self.min_nm,
                max_nm: self.max_nm,
                reason: "maximum wavelength must be finite and >= minimum wavelength",
            });
        }
        Ok(())
    }

    /// Checks if this range overlaps with another wavelength interval.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.min_nm <= other.max_nm && other.min_nm <= self.max_nm
    }
}

/// Spatial resolution or ground pixel scale (`SPACE-01`, `GEO-02`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpatialResolution {
    /// Ground sample distance (GSD) in metres per pixel for Earth or planetary surfaces.
    GroundSampleDistanceMeters(f64),
    /// Angular resolution in arcseconds per pixel for space and celestial telescopes.
    AngularResolutionArcsec(f64),
}

impl SpatialResolution {
    pub fn validate(&self) -> Result<(), SpaceImageryError> {
        match *self {
            Self::GroundSampleDistanceMeters(m) => {
                if !m.is_finite() || m <= 0.0 {
                    return Err(SpaceImageryError::InvalidResolution {
                        value: m,
                        reason: "ground sample distance must be positive and finite",
                    });
                }
            }
            Self::AngularResolutionArcsec(arcsec) => {
                if !arcsec.is_finite() || arcsec <= 0.0 {
                    return Err(SpaceImageryError::InvalidResolution {
                        value: arcsec,
                        reason: "angular resolution must be positive and finite",
                    });
                }
            }
        }
        Ok(())
    }
}

/// Target category and pointing parameters for space observations (`SPACE-01`, `A05`).
#[derive(Debug, Clone, PartialEq)]
pub enum SpaceObservationTarget {
    /// Terrestrial Earth-observation pointing over a WGS84 bounding envelope.
    EarthObservation {
        footprint: GeoBoundingBoxWgs84,
        region_label: Option<String>,
    },
    /// Deep-sky or celestial target with right ascension, declination, and FOV radius in degrees.
    Celestial {
        target_name: String,
        ra_deg: f64,
        dec_deg: f64,
        fov_radius_deg: f64,
    },
    /// Solar/heliospheric target (e.g. disk center, active region, coronal prominence).
    Solar {
        feature_label: String,
        active_region_no: Option<u32>,
    },
    /// Planetary or minor body mission target (e.g. Mars, Europa, Titan, Bennu).
    PlanetaryBody {
        body_name: String,
        sub_observer_lat_deg: Option<f64>,
        sub_observer_lon_deg: Option<f64>,
    },
}

impl SpaceObservationTarget {
    pub fn validate(&self) -> Result<(), SpaceImageryError> {
        match self {
            Self::EarthObservation {
                footprint,
                region_label,
            } => {
                footprint
                    .validate()
                    .map_err(|_e| SpaceImageryError::InvalidCoordinates {
                        reason: "invalid Earth observation bounding box",
                    })?;
                if let Some(l) = region_label {
                    validate_str("region_label", l)?;
                }
            }
            Self::Celestial {
                target_name,
                ra_deg,
                dec_deg,
                fov_radius_deg,
            } => {
                validate_str("target_name", target_name)?;
                if !ra_deg.is_finite() || !(0.0..360.0).contains(ra_deg) {
                    return Err(SpaceImageryError::InvalidCoordinates {
                        reason: "right ascension must be in [0.0, 360.0) degrees",
                    });
                }
                if !dec_deg.is_finite() || !(-90.0..=90.0).contains(dec_deg) {
                    return Err(SpaceImageryError::InvalidCoordinates {
                        reason: "declination must be in [-90.0, 90.0] degrees",
                    });
                }
                if !fov_radius_deg.is_finite() || *fov_radius_deg <= 0.0 || *fov_radius_deg > 180.0
                {
                    return Err(SpaceImageryError::InvalidCoordinates {
                        reason: "field of view radius must be in (0.0, 180.0] degrees",
                    });
                }
            }
            Self::Solar { feature_label, .. } => {
                validate_str("feature_label", feature_label)?;
            }
            Self::PlanetaryBody {
                body_name,
                sub_observer_lat_deg,
                sub_observer_lon_deg,
            } => {
                validate_str("body_name", body_name)?;
                if let Some(lat) = sub_observer_lat_deg
                    && (!lat.is_finite() || !(-90.0..=90.0).contains(lat))
                {
                    return Err(SpaceImageryError::InvalidCoordinates {
                        reason: "sub-observer latitude must be in [-90.0, 90.0] degrees",
                    });
                }
                if let Some(lon) = sub_observer_lon_deg
                    && (!lon.is_finite() || !(-180.0..=360.0).contains(lon))
                {
                    return Err(SpaceImageryError::InvalidCoordinates {
                        reason: "sub-observer longitude must be finite in [-180.0, 360.0] degrees",
                    });
                }
            }
        }
        Ok(())
    }
}

/// Provider candidate identification (`SPACE-01`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceDataProvider {
    pub provider_name: String,
    pub archive_service: String,
    pub is_candidate_catalog_entry: bool,
}

impl SpaceDataProvider {
    pub fn new(
        provider_name: impl Into<String>,
        archive_service: impl Into<String>,
        is_candidate_catalog_entry: bool,
    ) -> Result<Self, SpaceImageryError> {
        let p = Self {
            provider_name: provider_name.into(),
            archive_service: archive_service.into(),
            is_candidate_catalog_entry,
        };
        p.validate()?;
        Ok(p)
    }

    pub fn validate(&self) -> Result<(), SpaceImageryError> {
        validate_str("provider_name", &self.provider_name)?;
        validate_str("archive_service", &self.archive_service)?;
        Ok(())
    }
}

/// Calibration and processing level of scientific space data products (`SPACE-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CalibrationLevel {
    /// Level 0: Raw reconstructed unedited telemetry packets.
    Level0RawTelemetry,
    /// Level 1A: Reconstructed instrument counts with radiometric calibration applied.
    Level1ARadiometric,
    /// Level 1B: Calibrated radiance with geometric sensor pointing and WCS world coordinates attached.
    Level1BGeoreferenced,
    /// Level 2: Derived geophysical or astronomical parameters at native resolution.
    Level2Geophysical,
    /// Level 3: Spatially resampled, uniform gridded map or multi-frame mosaic.
    Level3GriddedComposite,
    /// Level 4: Model output or multi-instrument assimilated product.
    Level4ModelAssimilated,
}

impl fmt::Display for CalibrationLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Level0RawTelemetry => write!(f, "Level-0 Raw Telemetry"),
            Self::Level1ARadiometric => write!(f, "Level-1A Radiometric"),
            Self::Level1BGeoreferenced => write!(f, "Level-1B Georeferenced"),
            Self::Level2Geophysical => write!(f, "Level-2 Geophysical"),
            Self::Level3GriddedComposite => write!(f, "Level-3 Gridded Composite"),
            Self::Level4ModelAssimilated => write!(f, "Level-4 Model Assimilated"),
        }
    }
}

/// Stacking and frame integration method (`SPACE-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackingMethod {
    SingleExposure,
    MedianStack,
    MeanWithOutlierRejection,
    MinMaxClipped,
    ExposureWeighted,
    SummedContinuum,
}

/// Stacking and mosaic integration history (`SPACE-02`).
#[derive(Debug, Clone, PartialEq)]
pub struct StackingHistory {
    pub input_frame_count: usize,
    pub method: StackingMethod,
    pub alignment_residual_error: f64,
}

impl StackingHistory {
    pub fn new(
        input_frame_count: usize,
        method: StackingMethod,
        alignment_residual_error: f64,
    ) -> Result<Self, SpaceImageryError> {
        let s = Self {
            input_frame_count,
            method,
            alignment_residual_error,
        };
        s.validate(usize::MAX)?;
        Ok(s)
    }

    pub fn validate(&self, max_inputs: usize) -> Result<(), SpaceImageryError> {
        if self.input_frame_count == 0 {
            return Err(SpaceImageryError::InvalidExposureDuration {
                seconds: 0.0,
                reason: "stacking history must have at least 1 input frame",
            });
        }
        if self.input_frame_count > max_inputs {
            return Err(SpaceImageryError::BudgetExceeded {
                resource: "stacking_input_frames",
                limit: max_inputs,
            });
        }
        if !self.alignment_residual_error.is_finite() || self.alignment_residual_error < 0.0 {
            return Err(SpaceImageryError::InvalidResolution {
                value: self.alignment_residual_error,
                reason: "alignment residual error must be non-negative and finite",
            });
        }
        Ok(())
    }
}

/// Scientific product file format (`SPACE-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScientificProductFormat {
    FitsPrimaryImage,
    FitsMultiExtensionTable,
    GeoTiffCalibratedRadiance,
    Hdf5Eos,
    NetCdf4,
    RawCcdArray,
}

/// Raw scientific product record (`SPACE-02`, `A05`).
#[derive(Debug, Clone, PartialEq)]
pub struct ScientificProductRecord {
    pub product_id: String,
    pub format: ScientificProductFormat,
    pub calibration_level: CalibrationLevel,
    pub exposure_time_seconds: f64,
    pub source_version: String,
    pub stacking_history: Option<StackingHistory>,
    pub publication_reference: Option<String>,
    pub scientific_data_uri: Option<String>,
    pub checksum_sha256: Option<String>,
}

impl ScientificProductRecord {
    pub fn new(
        product_id: impl Into<String>,
        format: ScientificProductFormat,
        calibration_level: CalibrationLevel,
        exposure_time_seconds: f64,
        source_version: impl Into<String>,
    ) -> Result<Self, SpaceImageryError> {
        let rec = Self {
            product_id: product_id.into(),
            format,
            calibration_level,
            exposure_time_seconds,
            source_version: source_version.into(),
            stacking_history: None,
            publication_reference: None,
            scientific_data_uri: None,
            checksum_sha256: None,
        };
        rec.validate(usize::MAX)?;
        Ok(rec)
    }

    pub fn validate(&self, max_stack_inputs: usize) -> Result<(), SpaceImageryError> {
        validate_str("product_id", &self.product_id)?;
        validate_str("source_version", &self.source_version)?;
        if !self.exposure_time_seconds.is_finite() || self.exposure_time_seconds <= 0.0 {
            return Err(SpaceImageryError::InvalidExposureDuration {
                seconds: self.exposure_time_seconds,
                reason: "exposure time must be positive and finite",
            });
        }
        if let Some(ref sh) = self.stacking_history {
            sh.validate(max_stack_inputs)?;
        }
        if let Some(ref pub_ref) = self.publication_reference {
            validate_str("publication_reference", pub_ref)?;
        }
        if let Some(ref uri) = self.scientific_data_uri {
            validate_str("scientific_data_uri", uri)?;
        }
        if let Some(ref hash) = self.checksum_sha256 {
            validate_str("checksum_sha256", hash)?;
        }
        Ok(())
    }
}

/// Color mapping and presentation category (`SPACE-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorMapping {
    /// True color: mapped directly from natural spectral RGB bands.
    TrueColor,
    /// False color: explicit band-to-channel mapping (e.g. Red=SWIR, Green=NIR, Blue=Red).
    FalseColor {
        red_channel_source: String,
        green_channel_source: String,
        blue_channel_source: String,
    },
    /// Single band greyscale with explicit band or filter identifier.
    SingleBandGreyscale { band_name: String },
    /// Outreach aesthetic rendering with artistic enhancement or tone curves (`SPACE-02`).
    OutreachRendering {
        renderer_attribution: String,
        enhancement_notes: String,
    },
    /// Illustrative simulation or computer-generated model (`AGENTS.md` §2).
    IllustrativeSimulation { generator_model: String },
}

impl ColorMapping {
    /// Returns true if this presentation is an outreach aesthetic rendering or simulation.
    #[must_use]
    pub fn is_outreach_or_simulated(&self) -> bool {
        matches!(
            self,
            Self::OutreachRendering { .. } | Self::IllustrativeSimulation { .. }
        )
    }

    /// Returns a short machine-readable classification code.
    #[must_use]
    pub fn classification_code(&self) -> &'static str {
        match self {
            Self::TrueColor => "TRUE_COLOR",
            Self::FalseColor { .. } => "FALSE_COLOR",
            Self::SingleBandGreyscale { .. } => "SINGLE_BAND_GREYSCALE",
            Self::OutreachRendering { .. } => "OUTREACH_RENDERING",
            Self::IllustrativeSimulation { .. } => "ILLUSTRATIVE_SIMULATION",
        }
    }

    pub fn validate(&self) -> Result<(), SpaceImageryError> {
        match self {
            Self::TrueColor => Ok(()),
            Self::FalseColor {
                red_channel_source,
                green_channel_source,
                blue_channel_source,
            } => {
                validate_str("red_channel_source", red_channel_source)?;
                validate_str("green_channel_source", green_channel_source)?;
                validate_str("blue_channel_source", blue_channel_source)?;
                Ok(())
            }
            Self::SingleBandGreyscale { band_name } => validate_str("band_name", band_name),
            Self::OutreachRendering {
                renderer_attribution,
                enhancement_notes,
            } => {
                validate_str("renderer_attribution", renderer_attribution)?;
                validate_str("enhancement_notes", enhancement_notes)
            }
            Self::IllustrativeSimulation { generator_model } => {
                validate_str("generator_model", generator_model)
            }
        }
    }
}

/// Derived image preview record (`SPACE-02`).
#[derive(Debug, Clone, PartialEq)]
pub struct ImagePreviewRecord {
    pub preview_id: String,
    pub color_mapping: ColorMapping,
    pub width_px: u32,
    pub height_px: u32,
    pub mime_type: String,
    pub preview_uri: String,
}

impl ImagePreviewRecord {
    pub fn new(
        preview_id: impl Into<String>,
        color_mapping: ColorMapping,
        width_px: u32,
        height_px: u32,
        mime_type: impl Into<String>,
        preview_uri: impl Into<String>,
    ) -> Result<Self, SpaceImageryError> {
        let p = Self {
            preview_id: preview_id.into(),
            color_mapping,
            width_px,
            height_px,
            mime_type: mime_type.into(),
            preview_uri: preview_uri.into(),
        };
        p.validate()?;
        Ok(p)
    }

    pub fn validate(&self) -> Result<(), SpaceImageryError> {
        validate_str("preview_id", &self.preview_id)?;
        validate_str("mime_type", &self.mime_type)?;
        validate_str("preview_uri", &self.preview_uri)?;
        self.color_mapping.validate()?;
        if self.width_px == 0 || self.height_px == 0 {
            return Err(SpaceImageryError::InvalidResolution {
                value: 0.0,
                reason: "preview image dimensions must be non-zero",
            });
        }
        Ok(())
    }
}

/// Complete catalogued space observation entry (`SPACE-01`, `SPACE-02`).
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceObservationEntry {
    pub observation_id: String,
    pub mission: String,
    pub instrument: String,
    pub target: SpaceObservationTarget,
    pub spectral_regime: SpectralRegime,
    pub wavelength_range_nm: Option<WavelengthRangeNm>,
    pub time_window: TimeWindow,
    pub spatial_resolution: Option<SpatialResolution>,
    pub provider: SpaceDataProvider,
    pub scientific_product: ScientificProductRecord,
    pub previews: Vec<ImagePreviewRecord>,
}

impl SpaceObservationEntry {
    pub fn validate(&self, limits: &SpaceCatalogLimits) -> Result<(), SpaceImageryError> {
        validate_str("observation_id", &self.observation_id)?;
        validate_str("mission", &self.mission)?;
        validate_str("instrument", &self.instrument)?;
        self.target.validate()?;
        if let Some(ref w) = self.wavelength_range_nm {
            w.validate()?;
        }
        self.time_window
            .validate()
            .map_err(|_| SpaceImageryError::InvalidTimeWindow {
                reason: "observation time window invalid",
            })?;
        if let Some(ref res) = self.spatial_resolution {
            res.validate()?;
        }
        self.provider.validate()?;
        self.scientific_product.validate(limits.max_stack_inputs)?;
        if self.previews.len() > limits.max_previews_per_observation {
            return Err(SpaceImageryError::BudgetExceeded {
                resource: "observation_previews",
                limit: limits.max_previews_per_observation,
            });
        }
        for preview in &self.previews {
            preview.validate()?;
        }
        Ok(())
    }

    /// Evaluates if any attached preview is an outreach rendering or simulation.
    #[must_use]
    pub fn has_outreach_or_simulated_preview(&self) -> bool {
        self.previews
            .iter()
            .any(|p| p.color_mapping.is_outreach_or_simulated())
    }
}

/// Multi-criteria search filter for the space imagery catalog (`SPACE-01`, `GEO-02`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpaceObservationQuery {
    pub mission: Option<String>,
    pub instrument: Option<String>,
    pub spectral_regime: Option<SpectralRegime>,
    pub target_keyword: Option<String>,
    pub terrestrial_bounding_box: Option<GeoBoundingBoxWgs84>,
    pub time_window: Option<TimeWindow>,
    pub max_ground_sample_distance_m: Option<f64>,
    pub exclude_outreach_renderings: bool,
}

impl SpaceObservationQuery {
    /// Tests whether a candidate observation matches all configured query filters.
    #[must_use]
    pub fn matches(&self, entry: &SpaceObservationEntry) -> bool {
        if let Some(ref m) = self.mission
            && !entry.mission.eq_ignore_ascii_case(m)
        {
            return false;
        }
        if let Some(ref inst) = self.instrument
            && !entry.instrument.eq_ignore_ascii_case(inst)
        {
            return false;
        }
        if let Some(regime) = self.spectral_regime
            && entry.spectral_regime != regime
        {
            return false;
        }
        if let Some(ref kw) = self.target_keyword {
            let kw_lower = kw.to_ascii_lowercase();
            let matches_target = match &entry.target {
                SpaceObservationTarget::EarthObservation { region_label, .. } => region_label
                    .as_ref()
                    .is_some_and(|l| l.to_ascii_lowercase().contains(&kw_lower)),
                SpaceObservationTarget::Celestial { target_name, .. } => {
                    target_name.to_ascii_lowercase().contains(&kw_lower)
                }
                SpaceObservationTarget::Solar { feature_label, .. } => {
                    feature_label.to_ascii_lowercase().contains(&kw_lower)
                }
                SpaceObservationTarget::PlanetaryBody { body_name, .. } => {
                    body_name.to_ascii_lowercase().contains(&kw_lower)
                }
            };
            if !matches_target {
                return false;
            }
        }
        if let Some(ref bbox) = self.terrestrial_bounding_box {
            match &entry.target {
                SpaceObservationTarget::EarthObservation { footprint, .. } => {
                    if !bbox.overlaps_box(footprint) {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        if let Some(ref win) = self.time_window
            && !win.overlaps(&entry.time_window)
        {
            return false;
        }
        if let Some(max_gsd) = self.max_ground_sample_distance_m {
            match entry.spatial_resolution {
                Some(SpatialResolution::GroundSampleDistanceMeters(gsd)) => {
                    if gsd > max_gsd {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        if self.exclude_outreach_renderings && entry.has_outreach_or_simulated_preview() {
            return false;
        }
        true
    }
}

/// Spacecraft or mission feed stream classification (`SPACE-03`).
#[derive(Debug, Clone, PartialEq)]
pub enum StreamClassification {
    /// Live unaltered camera imagery from orbit or spacecraft.
    LiveCamera,
    /// Delayed transmission due to speed-of-light or buffer transit.
    DelayedTransmission {
        delay_seconds: f64,
        delay_reason: String,
    },
    /// Replay of historical or previously recorded mission activity.
    ReplayArchive { original_event_utc_ms: i64 },
    /// Studio commentary broadcast or narrated program with graphics overlays.
    CommentaryBroadcast,
    /// Computer-generated simulation, CAD model animation, or recreation.
    SimulationOrRecreation { engine: String },
}

impl StreamClassification {
    /// Returns true if this stream is a live camera view.
    #[must_use]
    pub fn is_live(&self) -> bool {
        matches!(self, Self::LiveCamera)
    }

    /// Returns a short classification token.
    #[must_use]
    pub fn classification_token(&self) -> &'static str {
        match self {
            Self::LiveCamera => "LIVE_CAMERA",
            Self::DelayedTransmission { .. } => "DELAYED_TRANSMISSION",
            Self::ReplayArchive { .. } => "REPLAY_ARCHIVE",
            Self::CommentaryBroadcast => "COMMENTARY_BROADCAST",
            Self::SimulationOrRecreation { .. } => "SIMULATION_OR_RECREATION",
        }
    }

    pub fn validate(&self) -> Result<(), SpaceImageryError> {
        match self {
            Self::LiveCamera => Ok(()),
            Self::DelayedTransmission {
                delay_seconds,
                delay_reason,
            } => {
                if !delay_seconds.is_finite() || *delay_seconds < 0.0 {
                    return Err(SpaceImageryError::InvalidTimeWindow {
                        reason: "delay seconds must be finite and non-negative",
                    });
                }
                validate_str("delay_reason", delay_reason)
            }
            Self::ReplayArchive { .. } => Ok(()),
            Self::CommentaryBroadcast => Ok(()),
            Self::SimulationOrRecreation { engine } => validate_str("engine", engine),
        }
    }
}

/// Current operating status of a public mission stream feed (`SPACE-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissionFeedStatus {
    NominalActive,
    LossOfSignal,
    EclipseNightPass,
    StationMaintenance,
    Terminated,
}

/// Bookmark on a mission stream feed (`SPACE-03`).
///
/// Pins source feed identity, stream classification interpretation, and explicit
/// distinct display vs. derived event timestamps with checked arithmetic.
#[derive(Debug, Clone, PartialEq)]
pub struct MissionFeedBookmark {
    pub bookmark_id: String,
    pub source_feed_id: String,
    pub stream_classification: StreamClassification,
    pub display_utc_ms: i64,
    pub estimated_event_utc_ms: i64,
    pub declared_clock_offset_ms: i64,
    pub timing_uncertainty_ms: u64,
    pub description: String,
    pub source_limitations: String,
}

impl MissionFeedBookmark {
    pub fn validate(&self) -> Result<(), SpaceImageryError> {
        validate_str("bookmark_id", &self.bookmark_id)?;
        validate_str("source_feed_id", &self.source_feed_id)?;
        validate_str("description", &self.description)?;
        validate_str("source_limitations", &self.source_limitations)?;
        self.stream_classification.validate()?;
        Ok(())
    }
}

/// Mission or space station feed record (`SPACE-03`).
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceMissionFeed {
    pub feed_id: String,
    pub mission_name: String,
    pub feed_title: String,
    pub classification: StreamClassification,
    pub status: MissionFeedStatus,
    pub declared_clock_offset_ms: i64,
    pub bookmarks: Vec<MissionFeedBookmark>,
}

impl SpaceMissionFeed {
    pub fn new(
        feed_id: impl Into<String>,
        mission_name: impl Into<String>,
        feed_title: impl Into<String>,
        classification: StreamClassification,
        status: MissionFeedStatus,
        declared_clock_offset_ms: i64,
    ) -> Result<Self, SpaceImageryError> {
        let f = Self {
            feed_id: feed_id.into(),
            mission_name: mission_name.into(),
            feed_title: feed_title.into(),
            classification,
            status,
            declared_clock_offset_ms,
            bookmarks: Vec::new(),
        };
        validate_str("feed_id", &f.feed_id)?;
        validate_str("mission_name", &f.mission_name)?;
        validate_str("feed_title", &f.feed_title)?;
        f.classification.validate()?;
        Ok(f)
    }

    pub fn validate(&self, limits: &SpaceCatalogLimits) -> Result<(), SpaceImageryError> {
        validate_str("feed_id", &self.feed_id)?;
        validate_str("mission_name", &self.mission_name)?;
        validate_str("feed_title", &self.feed_title)?;
        self.classification.validate()?;
        if self.bookmarks.len() > limits.max_bookmarks_per_feed {
            return Err(SpaceImageryError::BudgetExceeded {
                resource: "feed_bookmarks",
                limit: limits.max_bookmarks_per_feed,
            });
        }
        for bookmark in &self.bookmarks {
            bookmark.validate()?;
            if bookmark.source_feed_id != self.feed_id {
                return Err(SpaceImageryError::InvalidFeedAssociation {
                    bookmark_feed_id: bookmark.source_feed_id.clone(),
                    host_feed_id: self.feed_id.clone(),
                });
            }
        }
        Ok(())
    }

    /// Creates a bookmark bound to this feed, preserving the stream classification snapshot,
    /// clock offset, and calculating estimated event time from display time using checked arithmetic.
    ///
    /// ### Timing Sign Convention and Invariants (`C1`)
    /// - `display_utc_ms`: UTC epoch timestamp at presentation/viewing.
    /// - `declared_clock_offset_ms`: Clock correction added to stream time (`event = display + offset`).
    /// - `DelayedTransmission`: Propagation delay in milliseconds $D = \text{round}(\text{delay\_seconds} \times 1000.0)$.
    ///   The physical event occurred earlier: `event = display - D + offset`.
    /// - Arithmetic is strictly checked against `i64` bounds; overflow returns `SpaceImageryError::TimestampOverflow`.
    pub fn create_bookmark(
        &self,
        bookmark_id: impl Into<String>,
        display_utc_ms: i64,
        description: impl Into<String>,
        timing_uncertainty_ms: u64,
        source_limitations: impl Into<String>,
    ) -> Result<MissionFeedBookmark, SpaceImageryError> {
        let estimated_event_utc_ms = match &self.classification {
            StreamClassification::DelayedTransmission { delay_seconds, .. } => {
                let delay_ms_f64 = (*delay_seconds * 1000.0).round();
                const TWO_POW_63: f64 = 9223372036854775808.0; // 2^63
                if !delay_ms_f64.is_finite() || delay_ms_f64 < 0.0 || delay_ms_f64 >= TWO_POW_63 {
                    return Err(SpaceImageryError::TimestampOverflow {
                        operation: "delay_seconds_to_ms",
                        reason: "propagation delay seconds conversion to millisecond integer overflowed",
                    });
                }
                let delay_ms = delay_ms_f64 as i64;
                if delay_ms < 0 || delay_ms == i64::MAX {
                    return Err(SpaceImageryError::TimestampOverflow {
                        operation: "delay_seconds_to_ms",
                        reason: "propagation delay seconds conversion to millisecond integer overflowed",
                    });
                }
                let event_before_offset = display_utc_ms.checked_sub(delay_ms).ok_or(
                    SpaceImageryError::TimestampOverflow {
                        operation: "display_utc_ms - delay_ms",
                        reason: "display timestamp minus delay underflowed i64",
                    },
                )?;
                event_before_offset
                    .checked_add(self.declared_clock_offset_ms)
                    .ok_or(SpaceImageryError::TimestampOverflow {
                        operation: "event_time + clock_offset",
                        reason: "event timestamp plus clock offset overflowed i64",
                    })?
            }
            StreamClassification::ReplayArchive {
                original_event_utc_ms,
            } => *original_event_utc_ms,
            StreamClassification::LiveCamera
            | StreamClassification::CommentaryBroadcast
            | StreamClassification::SimulationOrRecreation { .. } => display_utc_ms
                .checked_add(self.declared_clock_offset_ms)
                .ok_or(SpaceImageryError::TimestampOverflow {
                    operation: "display_utc_ms + clock_offset",
                    reason: "display timestamp plus clock offset overflowed i64",
                })?,
        };

        let bm = MissionFeedBookmark {
            bookmark_id: bookmark_id.into(),
            source_feed_id: self.feed_id.clone(),
            stream_classification: self.classification.clone(),
            display_utc_ms,
            estimated_event_utc_ms,
            declared_clock_offset_ms: self.declared_clock_offset_ms,
            timing_uncertainty_ms,
            description: description.into(),
            source_limitations: source_limitations.into(),
        };
        bm.validate()?;
        Ok(bm)
    }

    pub fn add_bookmark(
        &mut self,
        bookmark: MissionFeedBookmark,
        limits: &SpaceCatalogLimits,
    ) -> Result<(), SpaceImageryError> {
        bookmark.validate()?;
        if bookmark.source_feed_id != self.feed_id {
            return Err(SpaceImageryError::InvalidFeedAssociation {
                bookmark_feed_id: bookmark.source_feed_id,
                host_feed_id: self.feed_id.clone(),
            });
        }
        if self.bookmarks.len() >= limits.max_bookmarks_per_feed {
            return Err(SpaceImageryError::BudgetExceeded {
                resource: "feed_bookmarks",
                limit: limits.max_bookmarks_per_feed,
            });
        }
        self.bookmarks.push(bookmark);
        Ok(())
    }
}

/// Navigable observation footprint in a mosaic reconstruction (`GEO-04`).
#[derive(Debug, Clone, PartialEq)]
pub struct ObservationFootprint {
    pub observation_id: String,
    pub bounding_box: GeoBoundingBoxWgs84,
    pub ground_sample_distance_m: Option<f64>,
    pub observation_time_utc_ms: i64,
}

/// Mosaic coverage classification state (`GEO-04`, `A04`).
#[derive(Debug, Clone, PartialEq)]
pub enum MosaicCoverageAssessment {
    /// Verified complete coverage across the queried spatial envelope.
    CompleteVerifiedCoverage {
        observation_count: usize,
        min_resolution_m: Option<f64>,
        max_resolution_m: Option<f64>,
    },
    /// Partial coverage with identified gaps or missing scenes.
    ///
    /// Evaluates planar coordinate angular area fraction ($(\Delta\text{lat}^\circ \times \Delta\text{lon}^\circ)$)
    /// with latitude cut tolerance $1\times 10^{-7\circ}$ and longitude merge tolerance $1\times 10^{-9\circ}$.
    PartialCoverageWithGaps {
        contributing_observation_count: usize,
        uncovered_angular_area_fraction: f64,
        uncovered_bands_count: usize,
        diagnostic: String,
    },
    /// No coverage available within the requested bounding envelope.
    NoCoverageAvailable,
}

/// Navigable mosaic coverage analysis report (`GEO-04`).
#[derive(Debug, Clone, PartialEq)]
pub struct MosaicCoverageReport {
    pub query_envelope: GeoBoundingBoxWgs84,
    pub footprints: Vec<ObservationFootprint>,
    pub assessment: MosaicCoverageAssessment,
}

/// Geometric and illumination capture metadata for change detection (`INTEL-03`).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageryCaptureGeometry {
    pub sun_elevation_deg: f64,
    pub sun_azimuth_deg: f64,
    pub off_nadir_angle_deg: f64,
    pub day_of_year: u32,
}

impl ImageryCaptureGeometry {
    pub fn new(
        sun_elevation_deg: f64,
        sun_azimuth_deg: f64,
        off_nadir_angle_deg: f64,
        day_of_year: u32,
    ) -> Result<Self, SpaceImageryError> {
        let g = Self {
            sun_elevation_deg,
            sun_azimuth_deg,
            off_nadir_angle_deg,
            day_of_year,
        };
        g.validate()?;
        Ok(g)
    }

    pub fn validate(&self) -> Result<(), SpaceImageryError> {
        if !self.sun_elevation_deg.is_finite() || !(-90.0..=90.0).contains(&self.sun_elevation_deg)
        {
            return Err(SpaceImageryError::InvalidCoordinates {
                reason: "sun elevation must be finite and within [-90.0, 90.0] degrees",
            });
        }
        if !self.sun_azimuth_deg.is_finite() || !(0.0..360.0).contains(&self.sun_azimuth_deg) {
            return Err(SpaceImageryError::InvalidCoordinates {
                reason: "sun azimuth must be finite and within [0.0, 360.0) degrees",
            });
        }
        if !self.off_nadir_angle_deg.is_finite()
            || !(0.0..=90.0).contains(&self.off_nadir_angle_deg)
        {
            return Err(SpaceImageryError::InvalidCoordinates {
                reason: "off-nadir angle must be finite and within [0.0, 90.0] degrees",
            });
        }
        if !(1..=366).contains(&self.day_of_year) {
            return Err(SpaceImageryError::InvalidTimeWindow {
                reason: "day of year must be in 1..=366",
            });
        }
        Ok(())
    }
}

/// Confounding geometric or seasonal limitations on detected changes (`INTEL-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometricLimitation {
    SignificantSunElevationDifference,
    OffNadirPerspectiveParallax,
    SeasonalFoliageOrSnowDiscrepancy,
    ResolutionDisparity,
    IncompatibleOrUnknownResolution,
}

/// Review status for a detected change candidate (`INTEL-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeReviewStatus {
    Unreviewed,
    LikelyIlluminationArtifact,
    SeasonalVegetationShift,
    ConfirmedPhysicalChange,
    Inconclusive,
}

/// Change detection candidate between two observations (`INTEL-03`).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageryChangeCandidate {
    pub candidate_id: String,
    pub location: GeoPointWgs84,
    pub raw_anomaly_score: f64,
    pub is_calibrated_probability: bool,
    pub is_simulated: bool,
    pub trigger_rationale: String,
    pub limitations: Vec<GeometricLimitation>,
    pub review_status: ChangeReviewStatus,
}

/// Execution status for pairwise change analysis (`INTEL-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeAnalysisStatus {
    /// Geometry and observational compatibility evaluated; raster pixel change detection not performed.
    RasterDetectorNotPerformed,
    /// Explicitly labeled synthetic candidate generation for testing or demonstration.
    SimulatedDemonstration,
    /// Detector executed with pinned evidence.
    EvaluatedWithEvidence,
}

/// Pairwise baseline comparison report (`INTEL-03`).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageryChangeReport {
    pub baseline_observation_id: String,
    pub comparison_observation_id: String,
    pub footprint_intersections: Vec<GeoBoundingBoxWgs84>,
    pub sun_elevation_diff_deg: f64,
    pub off_nadir_diff_deg: f64,
    pub seasonal_day_gap: u32,
    pub resolution_ratio: Option<f64>,
    pub limitations: Vec<GeometricLimitation>,
    pub analysis_status: ChangeAnalysisStatus,
    pub candidates: Vec<ImageryChangeCandidate>,
}

impl ImageryChangeReport {
    /// Returns the single intersection bounding box if there is exactly one component, or None.
    #[must_use]
    pub fn primary_footprint_intersection(&self) -> Option<&GeoBoundingBoxWgs84> {
        if self.footprint_intersections.len() == 1 {
            self.footprint_intersections.first()
        } else {
            None
        }
    }
}

/// Standalone in-memory space imagery catalog and observation manager (`SPACE-01`–`SPACE-03`).
#[derive(Debug, Default)]
pub struct SpaceImageryCatalog {
    observations: Vec<SpaceObservationEntry>,
    feeds: Vec<SpaceMissionFeed>,
}

impl SpaceImageryCatalog {
    pub fn new() -> Self {
        Self {
            observations: Vec::new(),
            feeds: Vec::new(),
        }
    }

    /// Adds a validated space observation entry to the catalog.
    pub fn add_observation(
        &mut self,
        entry: SpaceObservationEntry,
        limits: &SpaceCatalogLimits,
    ) -> Result<(), SpaceImageryError> {
        entry.validate(limits)?;
        if self.observations.len() >= limits.max_observations {
            return Err(SpaceImageryError::BudgetExceeded {
                resource: "catalog_observations",
                limit: limits.max_observations,
            });
        }
        if self
            .observations
            .iter()
            .any(|o| o.observation_id == entry.observation_id)
        {
            return Err(SpaceImageryError::DuplicateObservationId {
                id: entry.observation_id,
            });
        }
        self.observations.push(entry);
        Ok(())
    }

    /// Registers a space mission feed.
    pub fn add_feed(
        &mut self,
        feed: SpaceMissionFeed,
        limits: &SpaceCatalogLimits,
    ) -> Result<(), SpaceImageryError> {
        feed.validate(limits)?;
        if self.feeds.len() >= limits.max_observations {
            return Err(SpaceImageryError::BudgetExceeded {
                resource: "mission_feeds",
                limit: limits.max_observations,
            });
        }
        if self.feeds.iter().any(|f| f.feed_id == feed.feed_id) {
            return Err(SpaceImageryError::DuplicateFeedId { id: feed.feed_id });
        }
        self.feeds.push(feed);
        Ok(())
    }

    /// Queries the catalog with multi-criteria filtering (`SPACE-01`, `GEO-02`).
    pub fn query(
        &self,
        query: &SpaceObservationQuery,
        limits: &SpaceCatalogLimits,
        cancel: Option<CancelToken<'_>>,
    ) -> Result<Vec<&SpaceObservationEntry>, SpaceImageryError> {
        let mut results = Vec::new();
        let mut cancel = cancel;

        for (idx, entry) in self.observations.iter().enumerate() {
            if idx % 64 == 0
                && let Some(ref mut c) = cancel
            {
                c().map_err(SpaceImageryError::Stopped)?;
            }
            if query.matches(entry) {
                if results.len() >= limits.max_query_results {
                    return Err(SpaceImageryError::BudgetExceeded {
                        resource: "query_results",
                        limit: limits.max_query_results,
                    });
                }
                results.push(entry);
            }
        }
        Ok(results)
    }

    /// Evaluates mosaic coverage for a terrestrial envelope using exact clipped rectangle union (`GEO-04`).
    ///
    /// Decomposes antimeridian-crossing envelopes and footprints into non-wrapped sub-intervals,
    /// preventing clamp panics and omission of wrapped coverage.
    pub fn evaluate_mosaic_coverage(
        &self,
        envelope: &GeoBoundingBoxWgs84,
        time_window: Option<&TimeWindow>,
        limits: &SpaceCatalogLimits,
    ) -> Result<MosaicCoverageReport, SpaceImageryError> {
        envelope
            .validate()
            .map_err(|_| SpaceImageryError::InvalidCoordinates {
                reason: "invalid mosaic query envelope",
            })?;

        let mut footprints = Vec::new();
        let mut min_res = f64::INFINITY;
        let mut max_res = f64::NEG_INFINITY;
        let mut has_any_res = false;

        for entry in &self.observations {
            if let Some(win) = time_window
                && !win.overlaps(&entry.time_window)
            {
                continue;
            }
            if let SpaceObservationTarget::EarthObservation { footprint, .. } = &entry.target
                && envelope.overlaps_box(footprint)
            {
                if footprints.len() >= limits.max_footprints_per_mosaic {
                    return Err(SpaceImageryError::BudgetExceeded {
                        resource: "mosaic_footprints",
                        limit: limits.max_footprints_per_mosaic,
                    });
                }
                let gsd = match entry.spatial_resolution {
                    Some(SpatialResolution::GroundSampleDistanceMeters(g)) => {
                        has_any_res = true;
                        if g < min_res {
                            min_res = g;
                        }
                        if g > max_res {
                            max_res = g;
                        }
                        Some(g)
                    }
                    _ => None,
                };
                footprints.push(ObservationFootprint {
                    observation_id: entry.observation_id.clone(),
                    bounding_box: *footprint,
                    ground_sample_distance_m: gsd,
                    observation_time_utc_ms: entry.time_window.start_utc_ms(),
                });
            }
        }

        let assessment = if footprints.is_empty() {
            MosaicCoverageAssessment::NoCoverageAvailable
        } else {
            let (uncovered_fraction, uncovered_bands) =
                evaluate_envelope_union_coverage(envelope, &footprints);

            if uncovered_fraction <= 1e-6 && uncovered_bands == 0 {
                MosaicCoverageAssessment::CompleteVerifiedCoverage {
                    observation_count: footprints.len(),
                    min_resolution_m: if has_any_res { Some(min_res) } else { None },
                    max_resolution_m: if has_any_res { Some(max_res) } else { None },
                }
            } else {
                MosaicCoverageAssessment::PartialCoverageWithGaps {
                    contributing_observation_count: footprints.len(),
                    uncovered_angular_area_fraction: uncovered_fraction,
                    uncovered_bands_count: uncovered_bands,
                    diagnostic: format!(
                        "Mosaic incomplete: {:.1}% of envelope uncovered across {} latitude slice gaps",
                        uncovered_fraction * 100.0,
                        uncovered_bands
                    ),
                }
            }
        };

        Ok(MosaicCoverageReport {
            query_envelope: *envelope,
            footprints,
            assessment,
        })
    }

    /// Performs pairwise geometry and observational compatibility comparison (`INTEL-03`).
    ///
    /// Revalidates input capture geometries at the analysis boundary and evaluates viewing geometry,
    /// seasonal gap, and resolution ratio. Without attached pixel detector outputs or pinned evidence,
    /// status is `RasterDetectorNotPerformed` and no change candidates or synthetic anomaly scores are fabricated.
    pub fn compare_baselines(
        &self,
        baseline_id: &str,
        comparison_id: &str,
        baseline_geom: &ImageryCaptureGeometry,
        comparison_geom: &ImageryCaptureGeometry,
        _limits: &SpaceCatalogLimits,
    ) -> Result<ImageryChangeReport, SpaceImageryError> {
        // C2: Revalidate both input capture geometries at analysis boundary
        baseline_geom.validate()?;
        comparison_geom.validate()?;

        let baseline = self
            .observations
            .iter()
            .find(|o| o.observation_id == baseline_id)
            .ok_or_else(|| SpaceImageryError::ObservationNotFound {
                id: baseline_id.to_string(),
            })?;
        let comparison = self
            .observations
            .iter()
            .find(|o| o.observation_id == comparison_id)
            .ok_or_else(|| SpaceImageryError::ObservationNotFound {
                id: comparison_id.to_string(),
            })?;

        let sun_elevation_diff_deg =
            (baseline_geom.sun_elevation_deg - comparison_geom.sun_elevation_deg).abs();
        let off_nadir_diff_deg =
            (baseline_geom.off_nadir_angle_deg - comparison_geom.off_nadir_angle_deg).abs();
        let seasonal_day_gap = baseline_geom
            .day_of_year
            .abs_diff(comparison_geom.day_of_year);

        let resolution_ratio = match (baseline.spatial_resolution, comparison.spatial_resolution) {
            (
                Some(SpatialResolution::GroundSampleDistanceMeters(b)),
                Some(SpatialResolution::GroundSampleDistanceMeters(c)),
            ) if b > 0.0 && c > 0.0 && b.is_finite() && c.is_finite() => {
                let ratio = c / b;
                if ratio.is_finite() && ratio > 0.0 {
                    Some(ratio)
                } else {
                    None
                }
            }
            (
                Some(SpatialResolution::AngularResolutionArcsec(b)),
                Some(SpatialResolution::AngularResolutionArcsec(c)),
            ) if b > 0.0 && c > 0.0 && b.is_finite() && c.is_finite() => {
                let ratio = c / b;
                if ratio.is_finite() && ratio > 0.0 {
                    Some(ratio)
                } else {
                    None
                }
            }
            _ => None,
        };

        let mut limitations = Vec::new();
        if sun_elevation_diff_deg > 15.0 {
            limitations.push(GeometricLimitation::SignificantSunElevationDifference);
        }
        if off_nadir_diff_deg > 10.0 {
            limitations.push(GeometricLimitation::OffNadirPerspectiveParallax);
        }
        if (61..305).contains(&seasonal_day_gap) {
            limitations.push(GeometricLimitation::SeasonalFoliageOrSnowDiscrepancy);
        }
        match resolution_ratio {
            Some(ratio) if (ratio - 1.0).abs() > 0.5 => {
                limitations.push(GeometricLimitation::ResolutionDisparity);
            }
            None => {
                limitations.push(GeometricLimitation::IncompatibleOrUnknownResolution);
            }
            _ => {}
        }

        let footprint_intersections = match (&baseline.target, &comparison.target) {
            (
                SpaceObservationTarget::EarthObservation { footprint: f1, .. },
                SpaceObservationTarget::EarthObservation { footprint: f2, .. },
            ) => compute_box_intersections(f1, f2),
            _ => Vec::new(),
        };

        Ok(ImageryChangeReport {
            baseline_observation_id: baseline_id.to_string(),
            comparison_observation_id: comparison_id.to_string(),
            footprint_intersections,
            sun_elevation_diff_deg,
            off_nadir_diff_deg,
            seasonal_day_gap,
            resolution_ratio,
            limitations,
            analysis_status: ChangeAnalysisStatus::RasterDetectorNotPerformed,
            candidates: Vec::new(),
        })
    }

    /// Accessor for catalogued observations.
    #[must_use]
    pub fn observations(&self) -> &[SpaceObservationEntry] {
        &self.observations
    }

    /// Accessor for registered feeds.
    #[must_use]
    pub fn feeds(&self) -> &[SpaceMissionFeed] {
        &self.feeds
    }

    /// Plans a multi-source terrestrial globe composite over a target envelope (`GEO-02`, `GEO-04`, `SPACE-01`, `SPACE-02`, `A04`, `A05`).
    pub fn plan_globe_composite(
        &self,
        request: &CompositeStitchRequest,
        limits: &SpaceCatalogLimits,
        cancel: Option<CancelToken<'_>>,
    ) -> Result<GlobeCompositePlan, SpaceImageryError> {
        plan_globe_composite(request, limits, cancel)
    }
}

/// Acquisition platform class for multi-source terrestrial composite planning (`GEO-02`, `SPACE-01`, `MEDIA-02`, `FIELD-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompositeSourcePlatform {
    /// Orbital Earth-observation satellite sensor (e.g., Sentinel-2, Landsat-8/9, Planet NICFI, WorldView/Capella Open Data).
    SatelliteEarthObservation,
    /// Crew-operated or external camera frame from an orbital station or spacecraft (e.g., ISS Gateway to Astronaut Photography).
    SpacecraftHandheldOrCrewFrame,
    /// National or regional photogrammetric aerial orthophoto (e.g., USGS NAIP/3DEP, Swisstopo SWISSIMAGE, IGN BD ORTHO, PDOK).
    AerialOrthophoto,
    /// Georeferenced photogrammetric UAV / drone orthomosaic (e.g., OpenAerialMap).
    CalibratedUavOrDroneOrtho,
    /// 3D-posed street-level or field camera frame intersected with terrain (e.g., Mapillary, Panoramax, KartaView).
    CalibratedGroundOrStreetFrame,
}

impl fmt::Display for CompositeSourcePlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SatelliteEarthObservation => write!(f, "Satellite Earth Observation"),
            Self::SpacecraftHandheldOrCrewFrame => write!(f, "Spacecraft Crew / Handheld Frame"),
            Self::AerialOrthophoto => write!(f, "Aerial Orthophoto"),
            Self::CalibratedUavOrDroneOrtho => write!(f, "Calibrated UAV / Drone Orthomosaic"),
            Self::CalibratedGroundOrStreetFrame => write!(f, "Calibrated Ground / Street Frame"),
        }
    }
}

/// Radiometric modality of a surface observation patch (`SPACE-01`, `SPACE-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfaceRadiometricClass {
    /// Natural visible RGB optical observation.
    OpticalTrueColor,
    /// True-color RGB fused with a higher-resolution native panchromatic band from the same pass.
    OpticalPansharpenedTrueColor,
    /// Single-band visible/NIR panchromatic optical reflectance.
    OpticalPanchromatic,
    /// Non-RGB multispectral band assignment (e.g., NIR-Red-Green or SWIR false color).
    OpticalMultispectralFalseColor,
    /// Active microwave Synthetic Aperture Radar (SAR) amplitude/backscatter (penetrates clouds; never relabeled as optical).
    SyntheticApertureRadar,
}

impl SurfaceRadiometricClass {
    /// Returns true if this radiometric class is a passive optical observation.
    #[must_use]
    pub fn is_optical(&self) -> bool {
        !matches!(self, Self::SyntheticApertureRadar)
    }

    /// Returns true if this radiometric class is a natural true-color optical observation.
    #[must_use]
    pub fn is_true_color_optical(&self) -> bool {
        matches!(
            self,
            Self::OpticalTrueColor | Self::OpticalPansharpenedTrueColor
        )
    }

    /// Checks whether this surface radiometric class is physically compatible with the given `SpectralRegime`.
    #[must_use]
    pub fn is_compatible_with_spectral_regime(&self, regime: SpectralRegime) -> bool {
        match self {
            Self::SyntheticApertureRadar => {
                matches!(regime, SpectralRegime::Microwave | SpectralRegime::Radio)
            }
            Self::OpticalTrueColor | Self::OpticalPansharpenedTrueColor => matches!(
                regime,
                SpectralRegime::OpticalVisible
                    | SpectralRegime::Multispectral
                    | SpectralRegime::Hyperspectral
            ),
            Self::OpticalPanchromatic => matches!(
                regime,
                SpectralRegime::OpticalVisible
                    | SpectralRegime::NearInfrared
                    | SpectralRegime::Multispectral
                    | SpectralRegime::Hyperspectral
            ),
            Self::OpticalMultispectralFalseColor => matches!(
                regime,
                SpectralRegime::OpticalVisible
                    | SpectralRegime::NearInfrared
                    | SpectralRegime::MidInfrared
                    | SpectralRegime::FarInfrared
                    | SpectralRegime::Multispectral
                    | SpectralRegime::Hyperspectral
            ),
        }
    }
}

/// Integrity, obscuration, or editing state of a candidate imagery patch (`GEO-04`, `SPACE-02`, `MEDIA-03`).
#[derive(Debug, Clone, PartialEq)]
pub enum PatchIntegrityState {
    /// Unedited native sensor pixels with measured cloud/haze fraction in `[0.0, 1.0]`.
    UneditedClearNative { cloud_or_haze_fraction: f64 },
    /// Deliberately blurred, pixelated, or redacted region (e.g., statutory security mask in national aerial tiles).
    /// `coarse_effective_gsd_m` is `None` when the provider does not publish the blur kernel scale.
    DeliberatelyBlurredOrRedacted {
        reason: String,
        coarse_effective_gsd_m: Option<f64>,
    },
    /// Raster resampled onto a finer pixel grid spacing than the sensor's true native optical resolution.
    /// Does not imply zero cloud cover; `measured_cloud_or_haze_fraction` must be explicitly provided
    /// to qualify for clean optical assignment (`None` represents unmeasured/unknown cloud cover).
    UpsampledBeyondNative {
        native_gsd_m: f64,
        served_grid_spacing_m: f64,
        measured_cloud_or_haze_fraction: Option<f64>,
    },
    /// Obscured by opaque cloud deck, dense cloud shadow, or smoke plume in `(0.0, 1.0]`.
    ObscuredByCloudShadowOrSmoke { obscuration_fraction: f64 },
    /// Missing sensor data wedge, scan-line corrector gap (e.g., Landsat-7 SLC-off), or detector saturation artifact.
    SensorSwathGapOrArtifact { artifact_description: String },
    /// Generative super-resolution or AI-inpainted texture synthesis (never treated as physical observation).
    PredictiveSuperResolutionOrInpainted {
        model_id: String,
        base_native_gsd_m: f64,
    },
}

impl PatchIntegrityState {
    pub fn validate(&self) -> Result<(), SpaceImageryError> {
        match self {
            Self::UneditedClearNative {
                cloud_or_haze_fraction,
            } => {
                if !cloud_or_haze_fraction.is_finite()
                    || !(0.0..=1.0).contains(cloud_or_haze_fraction)
                {
                    return Err(SpaceImageryError::InvalidPatchIntegrity {
                        reason: "cloud_or_haze_fraction must be finite and within [0.0, 1.0]",
                    });
                }
                Ok(())
            }
            Self::DeliberatelyBlurredOrRedacted {
                reason,
                coarse_effective_gsd_m,
            } => {
                validate_str("blur_or_redaction_reason", reason)?;
                if let Some(gsd) = coarse_effective_gsd_m
                    && (!gsd.is_finite() || *gsd <= 0.0)
                {
                    return Err(SpaceImageryError::InvalidPatchIntegrity {
                        reason: "coarse_effective_gsd_m for blurred/redacted patch must be finite and positive when present",
                    });
                }
                Ok(())
            }
            Self::UpsampledBeyondNative {
                native_gsd_m,
                served_grid_spacing_m,
                measured_cloud_or_haze_fraction,
            } => {
                if !native_gsd_m.is_finite()
                    || *native_gsd_m <= 0.0
                    || !served_grid_spacing_m.is_finite()
                    || *served_grid_spacing_m <= 0.0
                    || *served_grid_spacing_m >= *native_gsd_m
                {
                    return Err(SpaceImageryError::InvalidPatchIntegrity {
                        reason: "upsampled patch requires 0 < served_grid_spacing_m < native_gsd_m",
                    });
                }
                if let Some(cf) = measured_cloud_or_haze_fraction
                    && (!cf.is_finite() || !(0.0..=1.0).contains(cf))
                {
                    return Err(SpaceImageryError::InvalidPatchIntegrity {
                        reason: "measured_cloud_or_haze_fraction must be finite and within [0.0, 1.0] when present",
                    });
                }
                Ok(())
            }
            Self::ObscuredByCloudShadowOrSmoke {
                obscuration_fraction,
            } => {
                if !obscuration_fraction.is_finite()
                    || *obscuration_fraction <= 0.0
                    || *obscuration_fraction > 1.0
                {
                    return Err(SpaceImageryError::InvalidPatchIntegrity {
                        reason: "obscuration_fraction must be finite and within (0.0, 1.0]",
                    });
                }
                Ok(())
            }
            Self::SensorSwathGapOrArtifact {
                artifact_description,
            } => validate_str("artifact_description", artifact_description),
            Self::PredictiveSuperResolutionOrInpainted {
                model_id,
                base_native_gsd_m,
            } => {
                validate_str("model_id", model_id)?;
                if !base_native_gsd_m.is_finite() || *base_native_gsd_m <= 0.0 {
                    return Err(SpaceImageryError::InvalidPatchIntegrity {
                        reason: "base_native_gsd_m must be finite and positive",
                    });
                }
                Ok(())
            }
        }
    }

    fn severity_rank(&self) -> u8 {
        match self {
            Self::UneditedClearNative { .. } => 0,
            Self::UpsampledBeyondNative { .. } => 1,
            Self::DeliberatelyBlurredOrRedacted { .. } => 2,
            Self::ObscuredByCloudShadowOrSmoke { .. } => 3,
            Self::SensorSwathGapOrArtifact { .. } => 4,
            Self::PredictiveSuperResolutionOrInpainted { .. } => 5,
        }
    }
}

/// Localized polygon/bounding-box defect within an otherwise clean imagery patch (`GEO-04`, `SPACE-02`).
///
/// Models statutory blur zones inside high-resolution national aerial orthophotos, isolated
/// cumulus clouds/shadows, or localized sensor artifacts so the planner only selects alternate
/// sources over the affected cells while keeping the crisp surrounding pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalizedPatchDefect {
    pub defect_id: String,
    pub bounds: GeoBoundingBoxWgs84,
    pub defect_state: PatchIntegrityState,
}

impl LocalizedPatchDefect {
    pub fn new(
        defect_id: impl Into<String>,
        bounds: GeoBoundingBoxWgs84,
        defect_state: PatchIntegrityState,
    ) -> Result<Self, SpaceImageryError> {
        let d = Self {
            defect_id: defect_id.into(),
            bounds,
            defect_state,
        };
        d.validate()?;
        Ok(d)
    }

    pub fn validate(&self) -> Result<(), SpaceImageryError> {
        validate_str("defect_id", &self.defect_id)?;
        self.bounds
            .validate()
            .map_err(|_| SpaceImageryError::InvalidCoordinates {
                reason: "invalid localized defect bounding box",
            })?;
        self.defect_state.validate()?;
        if matches!(
            self.defect_state,
            PatchIntegrityState::UneditedClearNative { .. }
        ) {
            return Err(SpaceImageryError::InvalidPatchIntegrity {
                reason: "localized defect cannot declare UneditedClearNative state",
            });
        }
        Ok(())
    }
}

/// Vertical terrain reference surface used for orthorectification (`GEO-02`, `A04`).
#[derive(Debug, Clone, PartialEq)]
pub enum VerticalReference {
    /// Projected onto the smooth WGS84 reference ellipsoid without local terrain relief correction.
    EllipsoidWgs84,
    /// Orthorectified against a Digital Elevation/Surface Model (e.g., Copernicus DEM, USGS 3DEP LiDAR).
    OrthometricDem {
        dem_source: String,
        vertical_rmse_m: f64,
    },
    /// Unrectified perspective camera plane (subject to severe relief displacement on oblique views).
    UnrectifiedSensorPlane,
}

impl VerticalReference {
    pub fn validate(&self) -> Result<(), SpaceImageryError> {
        match self {
            Self::EllipsoidWgs84 | Self::UnrectifiedSensorPlane => Ok(()),
            Self::OrthometricDem {
                dem_source,
                vertical_rmse_m,
            } => {
                validate_str("dem_source", dem_source)?;
                if !vertical_rmse_m.is_finite() || *vertical_rmse_m < 0.0 {
                    return Err(SpaceImageryError::InvalidRegistration {
                        reason: "DEM vertical RMSE must be finite and non-negative",
                    });
                }
                Ok(())
            }
        }
    }

    /// Returns true if the vertical reference is an orthometric DEM.
    #[must_use]
    pub fn is_orthometric_dem(&self) -> bool {
        matches!(self, Self::OrthometricDem { .. })
    }
}

/// Geometric correspondence method pinning an image patch to WGS84 coordinates (`GEO-02`, `MEDIA-02`, `FIELD-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorrespondenceMethod {
    /// Orbital ephemeris / WCS rational polynomial coefficients (RPC) with DEM orthorectification.
    OrbitalWcsAndDemOrthorectification,
    /// Photogrammetric bundle adjustment tied to surveyed ground control points (`gcp_count >= 3`).
    GroundControlPointsBundleAdjustment { gcp_count: u32 },
    /// Calibrated 6-DoF camera intrinsics/extrinsics intersected with a 3D terrain mesh.
    CalibratedCameraPoseAndTerrainIntersection,
    /// Uncalibrated manual pin or EXIF-only approximate placement without geometric verification.
    UncalibratedApproximatePlacement,
}

impl CorrespondenceMethod {
    pub fn validate(&self) -> Result<(), SpaceImageryError> {
        if let Self::GroundControlPointsBundleAdjustment { gcp_count } = self
            && *gcp_count < 3
        {
            return Err(SpaceImageryError::InvalidRegistration {
                reason: "bundle adjustment requires at least 3 ground control points",
            });
        }
        Ok(())
    }

    /// Checks whether this correspondence method is physically applicable to the given `CompositeSourcePlatform`.
    #[must_use]
    pub fn is_compatible_with_platform(&self, platform: CompositeSourcePlatform) -> bool {
        match (platform, self) {
            (_, Self::UncalibratedApproximatePlacement) => false,
            (
                CompositeSourcePlatform::SatelliteEarthObservation,
                Self::OrbitalWcsAndDemOrthorectification
                | Self::GroundControlPointsBundleAdjustment { .. },
            ) => true,
            (
                CompositeSourcePlatform::SpacecraftHandheldOrCrewFrame,
                Self::GroundControlPointsBundleAdjustment { .. }
                | Self::CalibratedCameraPoseAndTerrainIntersection,
            ) => true,
            (
                CompositeSourcePlatform::AerialOrthophoto
                | CompositeSourcePlatform::CalibratedUavOrDroneOrtho,
                Self::GroundControlPointsBundleAdjustment { .. }
                | Self::CalibratedCameraPoseAndTerrainIntersection,
            ) => true,
            (
                CompositeSourcePlatform::CalibratedGroundOrStreetFrame,
                Self::CalibratedCameraPoseAndTerrainIntersection
                | Self::GroundControlPointsBundleAdjustment { .. },
            ) => true,
            _ => false,
        }
    }
}

/// Cross-view 3D spatial registration metadata for stitching heterogeneous imagery (`GEO-02`, `MEDIA-02`).
#[derive(Debug, Clone, PartialEq)]
pub struct CrossViewRegistration {
    pub vertical_reference: VerticalReference,
    pub correspondence_method: CorrespondenceMethod,
    pub horizontal_rmse_m: f64,
    pub off_nadir_angle_deg: f64,
}

impl CrossViewRegistration {
    pub fn new(
        vertical_reference: VerticalReference,
        correspondence_method: CorrespondenceMethod,
        horizontal_rmse_m: f64,
        off_nadir_angle_deg: f64,
    ) -> Result<Self, SpaceImageryError> {
        let r = Self {
            vertical_reference,
            correspondence_method,
            horizontal_rmse_m,
            off_nadir_angle_deg,
        };
        r.validate()?;
        Ok(r)
    }

    pub fn validate(&self) -> Result<(), SpaceImageryError> {
        self.vertical_reference.validate()?;
        self.correspondence_method.validate()?;
        if !self.horizontal_rmse_m.is_finite() || self.horizontal_rmse_m < 0.0 {
            return Err(SpaceImageryError::InvalidRegistration {
                reason: "horizontal registration RMSE must be finite and non-negative",
            });
        }
        if !self.off_nadir_angle_deg.is_finite()
            || !(0.0..=90.0).contains(&self.off_nadir_angle_deg)
        {
            return Err(SpaceImageryError::InvalidRegistration {
                reason: "off-nadir angle must be finite and within [0.0, 90.0] degrees",
            });
        }
        Ok(())
    }
}

/// Candidate imagery patch from a satellite, spacecraft crew camera, aerial survey, UAV, or ground source (`GEO-02`, `GEO-04`, `SPACE-01`, `SPACE-02`).
#[derive(Debug, Clone, PartialEq)]
pub struct CompositeCandidatePatch {
    pub patch_id: String,
    pub source_id: String,
    pub platform: CompositeSourcePlatform,
    pub provider: SpaceDataProvider,
    pub mission_or_collection: String,
    pub instrument_or_sensor: String,
    pub footprint: GeoBoundingBoxWgs84,
    pub time_window: TimeWindow,
    pub native_gsd_m: Option<f64>,
    pub served_grid_spacing_m: f64,
    pub radiometric_class: SurfaceRadiometricClass,
    pub calibration_level: CalibrationLevel,
    pub stacking_history: Option<StackingHistory>,
    pub color_mapping: Option<ColorMapping>,
    pub base_integrity: PatchIntegrityState,
    pub localized_defects: Vec<LocalizedPatchDefect>,
    pub registration: CrossViewRegistration,
}

impl CompositeCandidatePatch {
    pub fn validate(&self, limits: &SpaceCatalogLimits) -> Result<(), SpaceImageryError> {
        validate_str("patch_id", &self.patch_id)?;
        validate_str("source_id", &self.source_id)?;
        self.provider.validate()?;
        validate_str("mission_or_collection", &self.mission_or_collection)?;
        validate_str("instrument_or_sensor", &self.instrument_or_sensor)?;
        self.footprint
            .validate()
            .map_err(|_| SpaceImageryError::InvalidCoordinates {
                reason: "invalid candidate patch footprint bounding box",
            })?;
        self.time_window
            .validate()
            .map_err(|_| SpaceImageryError::InvalidTimeWindow {
                reason: "invalid candidate patch capture time window",
            })?;
        if let Some(native_gsd) = self.native_gsd_m
            && (!native_gsd.is_finite() || native_gsd <= 0.0)
        {
            return Err(SpaceImageryError::InvalidResolution {
                value: native_gsd,
                reason: "candidate native_gsd_m must be finite and positive when present",
            });
        }
        if !self.served_grid_spacing_m.is_finite() || self.served_grid_spacing_m <= 0.0 {
            return Err(SpaceImageryError::InvalidResolution {
                value: self.served_grid_spacing_m,
                reason: "candidate served_grid_spacing_m must be finite and positive",
            });
        }
        if let Some(ref sh) = self.stacking_history {
            sh.validate(limits.max_stack_inputs)?;
        }
        if let Some(ref cm) = self.color_mapping {
            cm.validate()?;
        }
        self.base_integrity.validate()?;
        self.registration.validate()?;

        if self.localized_defects.len() > limits.max_defect_regions_per_patch {
            return Err(SpaceImageryError::BudgetExceeded {
                resource: "patch_localized_defects",
                limit: limits.max_defect_regions_per_patch,
            });
        }
        for (idx, defect) in self.localized_defects.iter().enumerate() {
            defect.validate()?;
            if self.localized_defects[..idx]
                .iter()
                .any(|d| d.defect_id == defect.defect_id)
            {
                return Err(SpaceImageryError::DuplicateDefectId {
                    patch_id: self.patch_id.clone(),
                    defect_id: defect.defect_id.clone(),
                });
            }
        }
        Ok(())
    }

    /// Returns true if the raster grid was upsampled beyond its native optical resolution.
    #[must_use]
    pub fn was_upsampled_beyond_native(&self) -> bool {
        self.native_gsd_m
            .is_some_and(|native| self.served_grid_spacing_m < native)
            || matches!(
                self.base_integrity,
                PatchIntegrityState::UpsampledBeyondNative { .. }
            )
    }

    /// Collects all integrity states affecting `cell_bounds`: `base_integrity`, an explicit
    /// `UpsampledBeyondNative` state if `served_grid_spacing_m < native_gsd_m`, and every
    /// overlapping `LocalizedPatchDefect` (never dropping lower-ranked overlapping defects).
    #[must_use]
    pub fn applicable_integrity_states_for_cell(
        &self,
        cell_bounds: &GeoBoundingBoxWgs84,
    ) -> Vec<PatchIntegrityState> {
        let mut states = Vec::with_capacity(1 + self.localized_defects.len());
        states.push(self.base_integrity.clone());

        if let Some(native_gsd_m) = self.native_gsd_m
            && self.served_grid_spacing_m < native_gsd_m
            && !matches!(
                self.base_integrity,
                PatchIntegrityState::UpsampledBeyondNative { .. }
            )
        {
            let measured_cloud_or_haze_fraction = match self.base_integrity {
                PatchIntegrityState::UneditedClearNative {
                    cloud_or_haze_fraction,
                } => Some(cloud_or_haze_fraction),
                _ => None,
            };
            states.push(PatchIntegrityState::UpsampledBeyondNative {
                native_gsd_m,
                served_grid_spacing_m: self.served_grid_spacing_m,
                measured_cloud_or_haze_fraction,
            });
        }

        for defect in &self.localized_defects {
            if cell_bounds.overlaps_interior(&defect.bounds, 1e-9) {
                states.push(defect.defect_state.clone());
            }
        }

        states
    }

    /// Evaluates the primary (most severe) integrity state of this patch over `cell_bounds`,
    /// while `applicable_integrity_states_for_cell` retains all overlapping states.
    #[must_use]
    pub fn effective_integrity_for_cell(
        &self,
        cell_bounds: &GeoBoundingBoxWgs84,
    ) -> PatchIntegrityState {
        let states = self.applicable_integrity_states_for_cell(cell_bounds);
        select_primary_integrity_state(&states)
    }

    /// Computes the true verifiable ground sample distance in metres across all applicable
    /// `cell_states`, defending against interpolated tile pyramids (`max(native_gsd_m, served_grid_spacing_m)`)
    /// and applying all overlapping blur/redaction coarsening floors simultaneously.
    ///
    /// Returns `None` if the patch's native GSD is unknown (`None`) or if an overlapping
    /// `DeliberatelyBlurredOrRedacted` state has an unknown (`None`) coarse effective GSD.
    #[must_use]
    pub fn effective_verifiable_gsd_m(&self, cell_states: &[PatchIntegrityState]) -> Option<f64> {
        let mut gsd = match (self.native_gsd_m, &self.base_integrity) {
            (Some(native), _) => native.max(self.served_grid_spacing_m),
            (
                None,
                PatchIntegrityState::UpsampledBeyondNative {
                    native_gsd_m,
                    served_grid_spacing_m,
                    ..
                },
            ) => (*native_gsd_m)
                .max(*served_grid_spacing_m)
                .max(self.served_grid_spacing_m),
            (None, _) => return None,
        };

        for state in cell_states {
            match state {
                PatchIntegrityState::DeliberatelyBlurredOrRedacted {
                    coarse_effective_gsd_m: Some(coarse),
                    ..
                } => {
                    gsd = gsd.max(*coarse);
                }
                PatchIntegrityState::DeliberatelyBlurredOrRedacted {
                    coarse_effective_gsd_m: None,
                    ..
                } => {
                    return None;
                }
                PatchIntegrityState::PredictiveSuperResolutionOrInpainted {
                    base_native_gsd_m,
                    ..
                } => {
                    gsd = gsd.max(*base_native_gsd_m);
                }
                PatchIntegrityState::UpsampledBeyondNative {
                    native_gsd_m,
                    served_grid_spacing_m,
                    ..
                } => {
                    gsd = gsd.max(*native_gsd_m).max(*served_grid_spacing_m);
                }
                PatchIntegrityState::UneditedClearNative { .. }
                | PatchIntegrityState::ObscuredByCloudShadowOrSmoke { .. }
                | PatchIntegrityState::SensorSwathGapOrArtifact { .. } => {}
            }
        }

        Some(gsd)
    }

    /// Returns true if this patch represents a single instantaneous sensor acquisition rather
    /// than a multi-frame stack, gridded composite mosaic, or long time-integrated window.
    #[must_use]
    pub fn is_single_instant_acquisition(&self, max_single_instant_duration_ms: i64) -> bool {
        let duration_ms =
            i128::from(self.time_window.end_utc_ms()) - i128::from(self.time_window.start_utc_ms());
        if duration_ms > i128::from(max_single_instant_duration_ms) {
            return false;
        }
        if matches!(
            self.calibration_level,
            CalibrationLevel::Level3GriddedComposite | CalibrationLevel::Level4ModelAssimilated
        ) {
            return false;
        }
        if let Some(ref sh) = self.stacking_history
            && (sh.input_frame_count > 1 || sh.method != StackingMethod::SingleExposure)
        {
            return false;
        }
        true
    }
}

fn select_primary_integrity_state(states: &[PatchIntegrityState]) -> PatchIntegrityState {
    let mut best =
        states
            .first()
            .cloned()
            .unwrap_or(PatchIntegrityState::SensorSwathGapOrArtifact {
                artifact_description: "empty integrity state list".into(),
            });
    for state in states.iter().skip(1) {
        let cur_rank = best.severity_rank();
        let new_rank = state.severity_rank();
        if new_rank > cur_rank {
            best = state.clone();
        } else if new_rank == cur_rank {
            match (&best, state) {
                (
                    PatchIntegrityState::DeliberatelyBlurredOrRedacted {
                        coarse_effective_gsd_m: cur_gsd,
                        ..
                    },
                    PatchIntegrityState::DeliberatelyBlurredOrRedacted {
                        coarse_effective_gsd_m: new_gsd,
                        ..
                    },
                ) => {
                    let replace = match (cur_gsd, new_gsd) {
                        (Some(_), None) => true,
                        (Some(c), Some(n)) => n > c,
                        _ => false,
                    };
                    if replace {
                        best = state.clone();
                    }
                }
                (
                    PatchIntegrityState::ObscuredByCloudShadowOrSmoke {
                        obscuration_fraction: cur_obs,
                    },
                    PatchIntegrityState::ObscuredByCloudShadowOrSmoke {
                        obscuration_fraction: new_obs,
                    },
                ) if new_obs > cur_obs => {
                    best = state.clone();
                }
                (
                    PatchIntegrityState::UneditedClearNative {
                        cloud_or_haze_fraction: cur_cf,
                    },
                    PatchIntegrityState::UneditedClearNative {
                        cloud_or_haze_fraction: new_cf,
                    },
                ) if new_cf > cur_cf => {
                    best = state.clone();
                }
                _ => {}
            }
        }
    }
    best
}

impl SpaceObservationEntry {
    /// Converts a catalogued `EarthObservation` entry into a `CompositeCandidatePatch` for multi-source
    /// globe composite planning, validating that `radiometric_class` matches `spectral_regime`,
    /// preserving `stacking_history` and `spatial_resolution` (`None` when unknown), and never
    /// fabricating or taking `color_mapping` from an outreach preview when a scientific preview exists.
    #[allow(clippy::too_many_arguments)]
    pub fn to_composite_candidate_patch(
        &self,
        patch_id: impl Into<String>,
        platform: CompositeSourcePlatform,
        served_grid_spacing_m: f64,
        radiometric_class: SurfaceRadiometricClass,
        base_integrity: PatchIntegrityState,
        registration: CrossViewRegistration,
        limits: &SpaceCatalogLimits,
    ) -> Result<CompositeCandidatePatch, SpaceImageryError> {
        self.validate(limits)?;
        let footprint = match &self.target {
            SpaceObservationTarget::EarthObservation { footprint, .. } => *footprint,
            _ => {
                return Err(SpaceImageryError::InvalidCoordinates {
                    reason: "only EarthObservation entries can be converted to globe composite patches",
                });
            }
        };
        if !radiometric_class.is_compatible_with_spectral_regime(self.spectral_regime) {
            return Err(SpaceImageryError::InvalidRadiometricModality {
                reason: "candidate radiometric_class is incompatible with observation spectral_regime",
            });
        }
        let native_gsd_m = match self.spatial_resolution {
            Some(SpatialResolution::GroundSampleDistanceMeters(gsd)) => Some(gsd),
            None => None,
            Some(SpatialResolution::AngularResolutionArcsec(arcsec)) => {
                return Err(SpaceImageryError::InvalidResolution {
                    value: arcsec,
                    reason: "globe composite candidate requires GroundSampleDistanceMeters or None, not AngularResolutionArcsec",
                });
            }
        };

        // Never take color mapping from an outreach/simulated preview if a scientific preview exists,
        // and never invent TrueColor when previews are absent.
        let color_mapping = self
            .previews
            .iter()
            .find(|p| !p.color_mapping.is_outreach_or_simulated())
            .or_else(|| self.previews.first())
            .map(|p| p.color_mapping.clone());

        let patch = CompositeCandidatePatch {
            patch_id: patch_id.into(),
            source_id: self.observation_id.clone(),
            platform,
            provider: self.provider.clone(),
            mission_or_collection: self.mission.clone(),
            instrument_or_sensor: self.instrument.clone(),
            footprint,
            time_window: self.time_window,
            native_gsd_m,
            served_grid_spacing_m,
            radiometric_class,
            calibration_level: self.scientific_product.calibration_level,
            stacking_history: self.scientific_product.stacking_history.clone(),
            color_mapping,
            base_integrity,
            localized_defects: Vec::new(),
            registration,
        };
        patch.validate(limits)?;
        Ok(patch)
    }
}

/// Policy governing multi-source globe composite planning and unredacted source selection (`GEO-02`, `GEO-04`, `SPACE-02`, `MEDIA-03`).
///
/// **Explicit Scope Exclusions:**
/// 1. No deblurring, super-resolution, or inpainting of redacted or low-resolution patches is performed.
/// 2. No restricted, credentialed, or higher-tier access requests are modeled or issued.
#[derive(Debug, Clone, PartialEq)]
pub struct CompositeStitchPolicy {
    pub max_cloud_or_haze_fraction: f64,
    pub max_horizontal_registration_rmse_m: f64,
    pub max_off_nadir_angle_deg: f64,
    pub max_single_instant_duration_ms: i64,
    pub require_terrain_orthorectification_for_oblique: bool,
    pub require_true_color_optical: bool,
    pub allow_sar_cloud_penetration_fallback: bool,
    pub allow_degraded_fallback_when_no_clean_source: bool,
}

impl Default for CompositeStitchPolicy {
    fn default() -> Self {
        Self {
            max_cloud_or_haze_fraction: 0.15,
            max_horizontal_registration_rmse_m: 15.0,
            max_off_nadir_angle_deg: 35.0,
            max_single_instant_duration_ms: 300_000,
            require_terrain_orthorectification_for_oblique: true,
            require_true_color_optical: false,
            allow_sar_cloud_penetration_fallback: false,
            allow_degraded_fallback_when_no_clean_source: true,
        }
    }
}

impl CompositeStitchPolicy {
    pub fn validate(&self) -> Result<(), SpaceImageryError> {
        if !self.max_cloud_or_haze_fraction.is_finite()
            || !(0.0..=1.0).contains(&self.max_cloud_or_haze_fraction)
        {
            return Err(SpaceImageryError::InvalidCompositePolicy {
                reason: "max_cloud_or_haze_fraction must be finite and within [0.0, 1.0]",
            });
        }
        if !self.max_horizontal_registration_rmse_m.is_finite()
            || self.max_horizontal_registration_rmse_m <= 0.0
        {
            return Err(SpaceImageryError::InvalidCompositePolicy {
                reason: "max_horizontal_registration_rmse_m must be finite and positive",
            });
        }
        if !self.max_off_nadir_angle_deg.is_finite()
            || self.max_off_nadir_angle_deg <= 0.0
            || self.max_off_nadir_angle_deg > 90.0
        {
            return Err(SpaceImageryError::InvalidCompositePolicy {
                reason: "max_off_nadir_angle_deg must be finite and within (0.0, 90.0]",
            });
        }
        if self.max_single_instant_duration_ms <= 0 {
            return Err(SpaceImageryError::InvalidCompositePolicy {
                reason: "max_single_instant_duration_ms must be positive",
            });
        }
        Ok(())
    }
}

/// Request to plan a multi-source globe composite over a gridded WGS84 envelope (`GEO-02`, `GEO-04`).
#[derive(Debug, Clone, PartialEq)]
pub struct CompositeStitchRequest {
    pub target_envelope: GeoBoundingBoxWgs84,
    pub grid_rows: usize,
    pub grid_cols: usize,
    pub policy: CompositeStitchPolicy,
    pub candidates: Vec<CompositeCandidatePatch>,
}

impl CompositeStitchRequest {
    pub fn validate(&self, limits: &SpaceCatalogLimits) -> Result<(), SpaceImageryError> {
        self.target_envelope
            .validate()
            .map_err(|_| SpaceImageryError::InvalidCoordinates {
                reason: "invalid composite target envelope",
            })?;
        let lat_span = self.target_envelope.north_lat_deg() - self.target_envelope.south_lat_deg();
        let lon_span = if self.target_envelope.west_lon_deg() <= self.target_envelope.east_lon_deg()
        {
            self.target_envelope.east_lon_deg() - self.target_envelope.west_lon_deg()
        } else {
            (180.0 - self.target_envelope.west_lon_deg())
                + (self.target_envelope.east_lon_deg() - (-180.0))
        };
        if !lat_span.is_finite() || lat_span <= 0.0 || !lon_span.is_finite() || lon_span <= 0.0 {
            return Err(SpaceImageryError::InvalidCoordinates {
                reason: "composite target envelope must have positive non-zero area",
            });
        }
        if self.grid_rows == 0 || self.grid_cols == 0 {
            return Err(SpaceImageryError::InvalidCompositePolicy {
                reason: "composite grid_rows and grid_cols must be at least 1",
            });
        }
        let total_cells = self.grid_rows.checked_mul(self.grid_cols).ok_or(
            SpaceImageryError::BudgetExceeded {
                resource: "composite_grid_cells",
                limit: limits.max_composite_grid_cells,
            },
        )?;
        if total_cells > limits.max_composite_grid_cells {
            return Err(SpaceImageryError::BudgetExceeded {
                resource: "composite_grid_cells",
                limit: limits.max_composite_grid_cells,
            });
        }
        if self.candidates.len() > limits.max_composite_candidates {
            return Err(SpaceImageryError::BudgetExceeded {
                resource: "composite_candidates",
                limit: limits.max_composite_candidates,
            });
        }
        self.policy.validate()?;

        for (idx, candidate) in self.candidates.iter().enumerate() {
            candidate.validate(limits)?;
            if self.candidates[..idx]
                .iter()
                .any(|c| c.patch_id == candidate.patch_id)
            {
                return Err(SpaceImageryError::DuplicateCandidatePatchId {
                    id: candidate.patch_id.clone(),
                });
            }
        }
        Ok(())
    }
}

/// Auditable reason why an overlapping candidate patch was rejected or bypassed on a cell (`GEO-04`, `SPACE-02`).
#[derive(Debug, Clone, PartialEq)]
pub enum CandidateRejectionOrBypassReason {
    /// Rejected because color mapping is an outreach aesthetic rendering or simulation (`SPACE-02`).
    OutreachOrSimulatedPresentation,
    /// Rejected because pixels were produced by generative super-resolution or AI inpainting (`AGENTS.md` §2).
    PredictiveOrInpaintedContent { model_id: String },
    /// Rejected because the cell falls inside a sensor swath gap or saturation artifact.
    SensorSwathGapOrArtifact { artifact_description: String },
    /// Rejected because spatial placement is uncalibrated or lacks verified ground correspondence.
    UncalibratedSpatialCorrespondence,
    /// Rejected because the correspondence method does not fit the acquisition platform class.
    IncompatiblePlatformCorrespondence {
        platform: CompositeSourcePlatform,
        correspondence_method: CorrespondenceMethod,
    },
    /// Rejected because an oblique view, crew handheld frame, or ground/street frame lacks orthometric DEM terrain rectification.
    MissingTerrainOrthorectificationForObliqueView,
    /// Rejected because horizontal registration RMSE exceeds policy threshold.
    HorizontalRegistrationRmseExceeded {
        actual_rmse_m: f64,
        max_allowed_rmse_m: f64,
    },
    /// Rejected because off-nadir viewing angle exceeds policy threshold.
    OffNadirAngleExceeded {
        actual_deg: f64,
        max_allowed_deg: f64,
    },
    /// Rejected because the candidate footprint only partially overlaps the cell and does not cover it completely.
    InsufficientCellFootprintCoverage,
    /// Rejected because SAR radar fallback is disabled in the composite policy.
    RadarModalityDisabledByPolicy,
    /// Rejected because the policy requires true-color optical imagery (`require_true_color_optical = true`).
    NonTrueColorOpticalRejectedByPolicy {
        radiometric_class: SurfaceRadiometricClass,
    },
    /// Bypassed because the candidate is deliberately blurred or redacted in this cell.
    DeliberateBlurOrRedactionBypassed {
        reason: String,
        coarse_effective_gsd_m: Option<f64>,
    },
    /// Bypassed because cloud, shadow, smoke, or haze obscuration exceeds the clean optical policy limit (or is unknown).
    CloudOrHazeObscurationBypassed { obscuration_fraction: Option<f64> },
    /// Bypassed because another eligible candidate provided a cleaner tier or finer verifiable native resolution.
    OutrankedByFinerOrCleanerSource {
        winning_patch_id: String,
        winning_effective_gsd_m: Option<f64>,
        candidate_effective_gsd_m: Option<f64>,
    },
}

/// Receipt documenting why an overlapping candidate patch was not selected for a grid cell (`GEO-04`, `SPACE-02`).
///
/// Carries complete provenance (`source_id`, `mission_or_collection`, `time_window`) and all
/// `reasons` (including every overlapping defect on the cell) so no redaction or obscuration
/// history is lost even when `GlobeCompositePlan` is inspected without the input request.
#[derive(Debug, Clone, PartialEq)]
pub struct RejectedCandidateReceipt {
    pub patch_id: String,
    pub source_id: String,
    pub mission_or_collection: String,
    pub time_window: TimeWindow,
    pub reasons: Vec<CandidateRejectionOrBypassReason>,
}

/// Sensor modality of the winning patch assigned to a composite cell (`SPACE-01`, `SPACE-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignedModality {
    /// Passive optical reflectance (true color, pansharpened, panchromatic, or multispectral).
    OpticalReflectance,
    /// Active microwave SAR backscatter fallback (explicitly isolated from optical layers).
    RadarBackscatterFallback,
}

/// Provenance-pinned patch assignment for a single composite grid cell (`GEO-02`, `GEO-04`, `SPACE-02`).
#[derive(Debug, Clone, PartialEq)]
pub struct CompositeCellAssignment {
    pub patch_id: String,
    pub source_id: String,
    pub platform: CompositeSourcePlatform,
    pub provider_name: String,
    pub mission_or_collection: String,
    pub instrument_or_sensor: String,
    pub assigned_modality: AssignedModality,
    pub radiometric_class: SurfaceRadiometricClass,
    pub calibration_level: CalibrationLevel,
    pub stacking_history: Option<StackingHistory>,
    pub color_mapping: Option<ColorMapping>,
    pub effective_verifiable_gsd_m: Option<f64>,
    pub native_gsd_m: Option<f64>,
    pub served_grid_spacing_m: f64,
    pub was_upsampled_beyond_native: bool,
    pub time_window: TimeWindow,
    pub capture_midpoint_utc_ms: i64,
    pub registration: CrossViewRegistration,
    pub horizontal_rmse_m: f64,
    pub off_nadir_angle_deg: f64,
    pub cell_integrity: PatchIntegrityState,
    pub applicable_cell_states: Vec<PatchIntegrityState>,
    pub bypassed_degraded_patch_ids: Vec<String>,
}

/// Outcome of multi-source composite planning for a single grid cell (`GEO-04`, `A04`).
#[derive(Debug, Clone, PartialEq)]
pub enum CompositeCellOutcome {
    /// Cell is covered by an unedited, cloud-compliant optical observation at its verifiable native resolution.
    AssignedCleanOptical(CompositeCellAssignment),
    /// Cell had no clean optical coverage and was filled via SAR cloud-penetration backscatter fallback.
    AssignedRadarFallback(CompositeCellAssignment),
    /// Cell had no clean optical or SAR coverage; a blurred, cloud-degraded, or unknown-GSD patch was retained with an explicit warning.
    AssignedDegradedOpticalFallback {
        assignment: CompositeCellAssignment,
        degradation_warning: String,
    },
    /// No eligible candidate patch covers the full cell bounds.
    UncoveredGap { diagnostic: String },
}

impl CompositeCellOutcome {
    /// Returns the underlying cell assignment if any patch was assigned.
    #[must_use]
    pub fn assignment(&self) -> Option<&CompositeCellAssignment> {
        match self {
            Self::AssignedCleanOptical(a)
            | Self::AssignedRadarFallback(a)
            | Self::AssignedDegradedOpticalFallback { assignment: a, .. } => Some(a),
            Self::UncoveredGap { .. } => None,
        }
    }
}

/// Complete stitch plan and audit trail for a single grid cell (`GEO-04`).
#[derive(Debug, Clone, PartialEq)]
pub struct CompositeCellPlan {
    pub row: usize,
    pub col: usize,
    pub bounds: GeoBoundingBoxWgs84,
    pub outcome: CompositeCellOutcome,
    pub rejected_or_bypassed: Vec<RejectedCandidateReceipt>,
    pub omitted_rejected_receipts_count: usize,
}

/// Seam segment between two adjacent composite cells assigned to different patches (`GEO-04`, `A04`).
#[derive(Debug, Clone, PartialEq)]
pub struct CompositeSeamSegment {
    pub first_cell_row: usize,
    pub first_cell_col: usize,
    pub second_cell_row: usize,
    pub second_cell_col: usize,
    pub first_patch_id: String,
    pub second_patch_id: String,
    pub capture_time_delta_ms: i64,
    pub effective_gsd_ratio: Option<f64>,
    pub crosses_sensor_modality: bool,
    pub crosses_platform_class: bool,
}

/// Complete multi-source globe composite plan (`GEO-02`, `GEO-04`, `SPACE-01`, `SPACE-02`, `A04`, `A05`).
#[derive(Debug, Clone, PartialEq)]
pub struct GlobeCompositePlan {
    pub method_id: &'static str,
    pub policy: CompositeStitchPolicy,
    pub target_envelope: GeoBoundingBoxWgs84,
    pub grid_rows: usize,
    pub grid_cols: usize,
    pub cells: Vec<CompositeCellPlan>,
    pub seams: Vec<CompositeSeamSegment>,
    pub clean_optical_cells_count: usize,
    pub radar_fallback_cells_count: usize,
    pub retained_degraded_cells_count: usize,
    pub uncovered_cells_count: usize,
    pub cells_filled_from_alternate_optical_source: usize,
    pub cells_filled_from_radar_over_degraded_source: usize,
    pub total_omitted_rejected_receipts_count: usize,
    pub finest_verifiable_gsd_m: Option<f64>,
    pub coarsest_verifiable_gsd_m: Option<f64>,
    pub composite_capture_window: Option<TimeWindow>,
    pub is_single_instant_observation: bool,
}

/// Plans a multi-source globe composite across satellite, spacecraft crew, aerial, UAV, and ground imagery
/// by selecting unredacted, cloud-free passes where available without performing pixel deblurring,
/// super-resolution, or restricted access requests (`GEO-02`, `GEO-04`, `SPACE-01`, `SPACE-02`).
pub fn plan_globe_composite(
    request: &CompositeStitchRequest,
    limits: &SpaceCatalogLimits,
    cancel: Option<CancelToken<'_>>,
) -> Result<GlobeCompositePlan, SpaceImageryError> {
    request.validate(limits)?;

    let mut cancel = cancel;
    let total_cells = request.grid_rows * request.grid_cols;
    let mut cells = Vec::with_capacity(total_cells);

    let mut clean_optical_cells_count = 0usize;
    let mut radar_fallback_cells_count = 0usize;
    let mut retained_degraded_cells_count = 0usize;
    let mut uncovered_cells_count = 0usize;
    let mut cells_filled_from_alternate_optical_source = 0usize;
    let mut cells_filled_from_radar_over_degraded_source = 0usize;
    let mut remaining_receipt_budget = limits.max_composite_rejected_receipts;
    let mut total_omitted_rejected_receipts_count = 0usize;

    let mut finest_gsd = f64::INFINITY;
    let mut coarsest_gsd = f64::NEG_INFINITY;
    let mut has_assigned_gsd = false;

    let mut earliest_start_ms: Option<i64> = None;
    let mut latest_end_ms: Option<i64> = None;
    let mut unique_first_patch_id: Option<String> = None;
    let mut all_same_single_instant_patch = true;

    for row in 0..request.grid_rows {
        for col in 0..request.grid_cols {
            let cell_idx = row * request.grid_cols + col;
            if cell_idx.is_multiple_of(32)
                && let Some(ref mut c) = cancel
            {
                c().map_err(SpaceImageryError::Stopped)?;
            }

            let cell_bounds = compute_grid_cell_bounds(
                &request.target_envelope,
                row,
                col,
                request.grid_rows,
                request.grid_cols,
            )?;

            let cell_plan = evaluate_single_composite_cell(
                row,
                col,
                cell_bounds,
                &request.candidates,
                &request.policy,
                &mut remaining_receipt_budget,
            )?;
            total_omitted_rejected_receipts_count += cell_plan.omitted_rejected_receipts_count;

            match &cell_plan.outcome {
                CompositeCellOutcome::AssignedCleanOptical(a) => {
                    clean_optical_cells_count += 1;
                    if !a.bypassed_degraded_patch_ids.is_empty() {
                        cells_filled_from_alternate_optical_source += 1;
                    }
                    update_composite_stats(
                        a,
                        &request.policy,
                        &mut finest_gsd,
                        &mut coarsest_gsd,
                        &mut has_assigned_gsd,
                        &mut earliest_start_ms,
                        &mut latest_end_ms,
                        &mut unique_first_patch_id,
                        &mut all_same_single_instant_patch,
                    );
                }
                CompositeCellOutcome::AssignedRadarFallback(a) => {
                    radar_fallback_cells_count += 1;
                    if !a.bypassed_degraded_patch_ids.is_empty() {
                        cells_filled_from_radar_over_degraded_source += 1;
                    }
                    update_composite_stats(
                        a,
                        &request.policy,
                        &mut finest_gsd,
                        &mut coarsest_gsd,
                        &mut has_assigned_gsd,
                        &mut earliest_start_ms,
                        &mut latest_end_ms,
                        &mut unique_first_patch_id,
                        &mut all_same_single_instant_patch,
                    );
                }
                CompositeCellOutcome::AssignedDegradedOpticalFallback { assignment: a, .. } => {
                    retained_degraded_cells_count += 1;
                    update_composite_stats(
                        a,
                        &request.policy,
                        &mut finest_gsd,
                        &mut coarsest_gsd,
                        &mut has_assigned_gsd,
                        &mut earliest_start_ms,
                        &mut latest_end_ms,
                        &mut unique_first_patch_id,
                        &mut all_same_single_instant_patch,
                    );
                }
                CompositeCellOutcome::UncoveredGap { .. } => {
                    uncovered_cells_count += 1;
                    all_same_single_instant_patch = false;
                }
            }

            cells.push(cell_plan);
        }
    }

    let wraps_full_globe_lon = is_full_longitude_span(&request.target_envelope);
    let seams = compute_composite_seams(
        &cells,
        request.grid_rows,
        request.grid_cols,
        wraps_full_globe_lon,
    )?;

    let composite_capture_window = match (earliest_start_ms, latest_end_ms) {
        (Some(s), Some(e)) => {
            Some(
                TimeWindow::new(s, e).map_err(|_| SpaceImageryError::InvalidTimeWindow {
                    reason: "invalid composite capture time window",
                })?,
            )
        }
        _ => None,
    };

    let is_single_instant_observation = uncovered_cells_count == 0
        && !cells.is_empty()
        && all_same_single_instant_patch
        && unique_first_patch_id.is_some()
        && seams.is_empty();

    Ok(GlobeCompositePlan {
        method_id: COMPOSITE_PLAN_METHOD_ID,
        policy: request.policy.clone(),
        target_envelope: request.target_envelope,
        grid_rows: request.grid_rows,
        grid_cols: request.grid_cols,
        cells,
        seams,
        clean_optical_cells_count,
        radar_fallback_cells_count,
        retained_degraded_cells_count,
        uncovered_cells_count,
        cells_filled_from_alternate_optical_source,
        cells_filled_from_radar_over_degraded_source,
        total_omitted_rejected_receipts_count,
        finest_verifiable_gsd_m: if has_assigned_gsd {
            Some(finest_gsd)
        } else {
            None
        },
        coarsest_verifiable_gsd_m: if has_assigned_gsd {
            Some(coarsest_gsd)
        } else {
            None
        },
        composite_capture_window,
        is_single_instant_observation,
    })
}

#[allow(clippy::too_many_arguments)]
fn update_composite_stats(
    assignment: &CompositeCellAssignment,
    policy: &CompositeStitchPolicy,
    finest_gsd: &mut f64,
    coarsest_gsd: &mut f64,
    has_assigned_gsd: &mut bool,
    earliest_start_ms: &mut Option<i64>,
    latest_end_ms: &mut Option<i64>,
    unique_first_patch_id: &mut Option<String>,
    all_same_single_instant_patch: &mut bool,
) {
    if let Some(gsd) = assignment.effective_verifiable_gsd_m {
        *has_assigned_gsd = true;
        if gsd < *finest_gsd {
            *finest_gsd = gsd;
        }
        if gsd > *coarsest_gsd {
            *coarsest_gsd = gsd;
        }
    }
    let s = assignment.time_window.start_utc_ms();
    let e = assignment.time_window.end_utc_ms();
    *earliest_start_ms = Some(earliest_start_ms.map_or(s, |cur| cur.min(s)));
    *latest_end_ms = Some(latest_end_ms.map_or(e, |cur| cur.max(e)));

    let duration_ms = i128::from(e) - i128::from(s);
    if duration_ms > i128::from(policy.max_single_instant_duration_ms) {
        *all_same_single_instant_patch = false;
    }
    if matches!(
        assignment.calibration_level,
        CalibrationLevel::Level3GriddedComposite | CalibrationLevel::Level4ModelAssimilated
    ) {
        *all_same_single_instant_patch = false;
    }
    if let Some(ref sh) = assignment.stacking_history
        && (sh.input_frame_count > 1 || sh.method != StackingMethod::SingleExposure)
    {
        *all_same_single_instant_patch = false;
    }

    match unique_first_patch_id {
        None => *unique_first_patch_id = Some(assignment.patch_id.clone()),
        Some(first_id) => {
            if first_id != &assignment.patch_id {
                *all_same_single_instant_patch = false;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CandidateTier {
    CleanOptical = 0,
    RadarFallback = 1,
    DegradedOpticalFallback = 2,
}

struct EvaluatedCellCandidate<'a> {
    patch: &'a CompositeCandidatePatch,
    tier: CandidateTier,
    primary_cell_integrity: PatchIntegrityState,
    applicable_cell_states: Vec<PatchIntegrityState>,
    bypass_reasons: Vec<CandidateRejectionOrBypassReason>,
    effective_gsd_m: Option<f64>,
    obscuration_metric: f64,
    capture_midpoint_utc_ms: i64,
}

fn push_bounded_receipt(
    receipts: &mut Vec<RejectedCandidateReceipt>,
    omitted_count: &mut usize,
    remaining_budget: &mut usize,
    patch: &CompositeCandidatePatch,
    reasons: Vec<CandidateRejectionOrBypassReason>,
) {
    if *remaining_budget > 0 {
        *remaining_budget -= 1;
        receipts.push(RejectedCandidateReceipt {
            patch_id: patch.patch_id.clone(),
            source_id: patch.source_id.clone(),
            mission_or_collection: patch.mission_or_collection.clone(),
            time_window: patch.time_window,
            reasons,
        });
    } else {
        *omitted_count += 1;
    }
}

fn collect_cell_defect_reasons(
    cell_states: &[PatchIntegrityState],
    max_cloud_or_haze_fraction: f64,
) -> (
    Vec<CandidateRejectionOrBypassReason>,
    bool,
    bool,
    Option<f64>,
) {
    let mut reasons = Vec::new();
    let mut has_blur = false;
    let mut has_cloud_violation_or_unknown = false;
    let mut max_measured_obscuration: Option<f64> = None;

    for state in cell_states {
        match state {
            PatchIntegrityState::UneditedClearNative {
                cloud_or_haze_fraction,
            } => {
                max_measured_obscuration = Some(
                    max_measured_obscuration.map_or(*cloud_or_haze_fraction, |cur| {
                        cur.max(*cloud_or_haze_fraction)
                    }),
                );
                if *cloud_or_haze_fraction > max_cloud_or_haze_fraction {
                    has_cloud_violation_or_unknown = true;
                    reasons.push(
                        CandidateRejectionOrBypassReason::CloudOrHazeObscurationBypassed {
                            obscuration_fraction: Some(*cloud_or_haze_fraction),
                        },
                    );
                }
            }
            PatchIntegrityState::UpsampledBeyondNative {
                measured_cloud_or_haze_fraction,
                ..
            } => match measured_cloud_or_haze_fraction {
                Some(cf) => {
                    max_measured_obscuration =
                        Some(max_measured_obscuration.map_or(*cf, |cur| cur.max(*cf)));
                    if *cf > max_cloud_or_haze_fraction {
                        has_cloud_violation_or_unknown = true;
                        reasons.push(
                            CandidateRejectionOrBypassReason::CloudOrHazeObscurationBypassed {
                                obscuration_fraction: Some(*cf),
                            },
                        );
                    }
                }
                None => {
                    has_cloud_violation_or_unknown = true;
                    reasons.push(
                        CandidateRejectionOrBypassReason::CloudOrHazeObscurationBypassed {
                            obscuration_fraction: None,
                        },
                    );
                }
            },
            PatchIntegrityState::DeliberatelyBlurredOrRedacted {
                reason,
                coarse_effective_gsd_m,
            } => {
                has_blur = true;
                reasons.push(
                    CandidateRejectionOrBypassReason::DeliberateBlurOrRedactionBypassed {
                        reason: reason.clone(),
                        coarse_effective_gsd_m: *coarse_effective_gsd_m,
                    },
                );
            }
            PatchIntegrityState::ObscuredByCloudShadowOrSmoke {
                obscuration_fraction,
            } => {
                has_cloud_violation_or_unknown = true;
                max_measured_obscuration = Some(
                    max_measured_obscuration
                        .map_or(*obscuration_fraction, |cur| cur.max(*obscuration_fraction)),
                );
                reasons.push(
                    CandidateRejectionOrBypassReason::CloudOrHazeObscurationBypassed {
                        obscuration_fraction: Some(*obscuration_fraction),
                    },
                );
            }
            PatchIntegrityState::SensorSwathGapOrArtifact {
                artifact_description,
            } => {
                reasons.push(CandidateRejectionOrBypassReason::SensorSwathGapOrArtifact {
                    artifact_description: artifact_description.clone(),
                });
            }
            PatchIntegrityState::PredictiveSuperResolutionOrInpainted { model_id, .. } => {
                reasons.push(
                    CandidateRejectionOrBypassReason::PredictiveOrInpaintedContent {
                        model_id: model_id.clone(),
                    },
                );
            }
        }
    }

    (
        reasons,
        has_blur,
        has_cloud_violation_or_unknown,
        max_measured_obscuration,
    )
}

fn evaluate_single_composite_cell(
    row: usize,
    col: usize,
    cell_bounds: GeoBoundingBoxWgs84,
    candidates: &[CompositeCandidatePatch],
    policy: &CompositeStitchPolicy,
    remaining_receipt_budget: &mut usize,
) -> Result<CompositeCellPlan, SpaceImageryError> {
    let mut rejected_or_bypassed = Vec::new();
    let mut omitted_rejected_receipts_count = 0usize;
    let mut eligible = Vec::new();
    let mut degraded_or_blurred_covering_ids = Vec::new();

    for patch in candidates {
        if !cell_bounds.overlaps_interior(&patch.footprint, 1e-9) {
            continue;
        }

        // 1. Exact cell containment check (partial cell overlap cannot cover the cell)
        if !patch
            .footprint
            .contains_box_with_tolerance(&cell_bounds, 1e-9)
        {
            push_bounded_receipt(
                &mut rejected_or_bypassed,
                &mut omitted_rejected_receipts_count,
                remaining_receipt_budget,
                patch,
                vec![CandidateRejectionOrBypassReason::InsufficientCellFootprintCoverage],
            );
            continue;
        }

        // 2. Outreach / simulated presentation check (SPACE-02)
        if patch
            .color_mapping
            .as_ref()
            .is_some_and(ColorMapping::is_outreach_or_simulated)
        {
            push_bounded_receipt(
                &mut rejected_or_bypassed,
                &mut omitted_rejected_receipts_count,
                remaining_receipt_budget,
                patch,
                vec![CandidateRejectionOrBypassReason::OutreachOrSimulatedPresentation],
            );
            continue;
        }

        let applicable_cell_states = patch.applicable_integrity_states_for_cell(&cell_bounds);
        let primary_cell_integrity = select_primary_integrity_state(&applicable_cell_states);
        let (defect_reasons, has_blur, has_cloud_violation_or_unknown, max_measured_obscuration) =
            collect_cell_defect_reasons(&applicable_cell_states, policy.max_cloud_or_haze_fraction);

        // 3. Predictive / AI inpainted content check across ANY applicable state (AGENTS.md §2)
        if applicable_cell_states.iter().any(|s| {
            matches!(
                s,
                PatchIntegrityState::PredictiveSuperResolutionOrInpainted { .. }
            )
        }) {
            push_bounded_receipt(
                &mut rejected_or_bypassed,
                &mut omitted_rejected_receipts_count,
                remaining_receipt_budget,
                patch,
                defect_reasons,
            );
            continue;
        }

        // 4. Sensor swath gap or saturation artifact check across ANY applicable state
        if applicable_cell_states
            .iter()
            .any(|s| matches!(s, PatchIntegrityState::SensorSwathGapOrArtifact { .. }))
        {
            degraded_or_blurred_covering_ids.push(patch.patch_id.clone());
            push_bounded_receipt(
                &mut rejected_or_bypassed,
                &mut omitted_rejected_receipts_count,
                remaining_receipt_budget,
                patch,
                defect_reasons,
            );
            continue;
        }

        // 5. Spatial calibration, platform-correspondence fit, and terrain orthorectification check (GEO-02, MEDIA-02, B3)
        if patch.registration.correspondence_method
            == CorrespondenceMethod::UncalibratedApproximatePlacement
        {
            push_bounded_receipt(
                &mut rejected_or_bypassed,
                &mut omitted_rejected_receipts_count,
                remaining_receipt_budget,
                patch,
                vec![CandidateRejectionOrBypassReason::UncalibratedSpatialCorrespondence],
            );
            continue;
        }

        if !patch
            .registration
            .correspondence_method
            .is_compatible_with_platform(patch.platform)
        {
            push_bounded_receipt(
                &mut rejected_or_bypassed,
                &mut omitted_rejected_receipts_count,
                remaining_receipt_budget,
                patch,
                vec![
                    CandidateRejectionOrBypassReason::IncompatiblePlatformCorrespondence {
                        platform: patch.platform,
                        correspondence_method: patch.registration.correspondence_method.clone(),
                    },
                ],
            );
            continue;
        }

        // Crew handheld frames and ground/street frames ALWAYS require an OrthometricDem
        // vertical reference to be placed onto a map cell; oblique views (> 15 deg off-nadir)
        // also require OrthometricDem when policy.require_terrain_orthorectification_for_oblique is true.
        let is_crew_or_ground_platform = matches!(
            patch.platform,
            CompositeSourcePlatform::SpacecraftHandheldOrCrewFrame
                | CompositeSourcePlatform::CalibratedGroundOrStreetFrame
        );
        let requires_orthometric_dem = is_crew_or_ground_platform
            || (policy.require_terrain_orthorectification_for_oblique
                && patch.registration.off_nadir_angle_deg > 15.0);
        if requires_orthometric_dem && !patch.registration.vertical_reference.is_orthometric_dem() {
            push_bounded_receipt(
                &mut rejected_or_bypassed,
                &mut omitted_rejected_receipts_count,
                remaining_receipt_budget,
                patch,
                vec![
                    CandidateRejectionOrBypassReason::MissingTerrainOrthorectificationForObliqueView,
                ],
            );
            continue;
        }

        if patch.registration.horizontal_rmse_m > policy.max_horizontal_registration_rmse_m {
            push_bounded_receipt(
                &mut rejected_or_bypassed,
                &mut omitted_rejected_receipts_count,
                remaining_receipt_budget,
                patch,
                vec![
                    CandidateRejectionOrBypassReason::HorizontalRegistrationRmseExceeded {
                        actual_rmse_m: patch.registration.horizontal_rmse_m,
                        max_allowed_rmse_m: policy.max_horizontal_registration_rmse_m,
                    },
                ],
            );
            continue;
        }

        if patch.registration.off_nadir_angle_deg > policy.max_off_nadir_angle_deg {
            push_bounded_receipt(
                &mut rejected_or_bypassed,
                &mut omitted_rejected_receipts_count,
                remaining_receipt_budget,
                patch,
                vec![CandidateRejectionOrBypassReason::OffNadirAngleExceeded {
                    actual_deg: patch.registration.off_nadir_angle_deg,
                    max_allowed_deg: policy.max_off_nadir_angle_deg,
                }],
            );
            continue;
        }

        let effective_gsd_m = patch.effective_verifiable_gsd_m(&applicable_cell_states);
        let capture_midpoint_utc_ms = compute_window_midpoint_ms(&patch.time_window);

        // 6. Classify into CleanOptical, RadarFallback, or DegradedOpticalFallback
        if patch.radiometric_class == SurfaceRadiometricClass::SyntheticApertureRadar {
            if !policy.allow_sar_cloud_penetration_fallback {
                push_bounded_receipt(
                    &mut rejected_or_bypassed,
                    &mut omitted_rejected_receipts_count,
                    remaining_receipt_budget,
                    patch,
                    vec![CandidateRejectionOrBypassReason::RadarModalityDisabledByPolicy],
                );
                continue;
            }
            if has_blur || has_cloud_violation_or_unknown || effective_gsd_m.is_none() {
                degraded_or_blurred_covering_ids.push(patch.patch_id.clone());
                push_bounded_receipt(
                    &mut rejected_or_bypassed,
                    &mut omitted_rejected_receipts_count,
                    remaining_receipt_budget,
                    patch,
                    defect_reasons,
                );
            } else {
                eligible.push(EvaluatedCellCandidate {
                    patch,
                    tier: CandidateTier::RadarFallback,
                    primary_cell_integrity,
                    applicable_cell_states,
                    bypass_reasons: defect_reasons,
                    effective_gsd_m,
                    obscuration_metric: max_measured_obscuration.unwrap_or(0.0),
                    capture_midpoint_utc_ms,
                });
            }
            continue;
        }

        // Optical true-color policy filter
        let true_color_satisfied = !policy.require_true_color_optical
            || (patch.radiometric_class.is_true_color_optical()
                && !matches!(
                    patch.color_mapping,
                    Some(
                        ColorMapping::FalseColor { .. } | ColorMapping::SingleBandGreyscale { .. }
                    )
                ));
        if !true_color_satisfied {
            push_bounded_receipt(
                &mut rejected_or_bypassed,
                &mut omitted_rejected_receipts_count,
                remaining_receipt_budget,
                patch,
                vec![
                    CandidateRejectionOrBypassReason::NonTrueColorOpticalRejectedByPolicy {
                        radiometric_class: patch.radiometric_class,
                    },
                ],
            );
            continue;
        }

        // Optical candidate classification across ALL applicable cell states
        if !has_blur && !has_cloud_violation_or_unknown && effective_gsd_m.is_some() {
            eligible.push(EvaluatedCellCandidate {
                patch,
                tier: CandidateTier::CleanOptical,
                primary_cell_integrity,
                applicable_cell_states,
                bypass_reasons: Vec::new(),
                effective_gsd_m,
                obscuration_metric: max_measured_obscuration.unwrap_or(0.0),
                capture_midpoint_utc_ms,
            });
        } else {
            degraded_or_blurred_covering_ids.push(patch.patch_id.clone());
            if policy.allow_degraded_fallback_when_no_clean_source {
                let obscuration_metric = if has_blur {
                    1.0
                } else {
                    max_measured_obscuration.unwrap_or(1.0)
                };
                eligible.push(EvaluatedCellCandidate {
                    patch,
                    tier: CandidateTier::DegradedOpticalFallback,
                    primary_cell_integrity,
                    applicable_cell_states,
                    bypass_reasons: defect_reasons,
                    effective_gsd_m,
                    obscuration_metric,
                    capture_midpoint_utc_ms,
                });
            } else {
                push_bounded_receipt(
                    &mut rejected_or_bypassed,
                    &mut omitted_rejected_receipts_count,
                    remaining_receipt_budget,
                    patch,
                    defect_reasons,
                );
            }
        }
    }

    if eligible.is_empty() {
        return Ok(CompositeCellPlan {
            row,
            col,
            bounds: cell_bounds,
            outcome: CompositeCellOutcome::UncoveredGap {
                diagnostic: format!(
                    "No eligible imagery patch covers cell ({row}, {col}); {} overlapping candidates rejected",
                    rejected_or_bypassed.len() + omitted_rejected_receipts_count
                ),
            },
            rejected_or_bypassed,
            omitted_rejected_receipts_count,
        });
    }

    eligible.sort_by(compare_evaluated_candidates);

    let winner = &eligible[0];
    let bypassed_degraded_patch_ids = if winner.tier != CandidateTier::DegradedOpticalFallback {
        degraded_or_blurred_covering_ids
    } else {
        Vec::new()
    };

    for runner_up in &eligible[1..] {
        let mut reasons = runner_up.bypass_reasons.clone();
        reasons.push(
            CandidateRejectionOrBypassReason::OutrankedByFinerOrCleanerSource {
                winning_patch_id: winner.patch.patch_id.clone(),
                winning_effective_gsd_m: winner.effective_gsd_m,
                candidate_effective_gsd_m: runner_up.effective_gsd_m,
            },
        );
        push_bounded_receipt(
            &mut rejected_or_bypassed,
            &mut omitted_rejected_receipts_count,
            remaining_receipt_budget,
            runner_up.patch,
            reasons,
        );
    }

    let assigned_modality =
        if winner.patch.radiometric_class == SurfaceRadiometricClass::SyntheticApertureRadar {
            AssignedModality::RadarBackscatterFallback
        } else {
            AssignedModality::OpticalReflectance
        };

    let assignment = CompositeCellAssignment {
        patch_id: winner.patch.patch_id.clone(),
        source_id: winner.patch.source_id.clone(),
        platform: winner.patch.platform,
        provider_name: winner.patch.provider.provider_name.clone(),
        mission_or_collection: winner.patch.mission_or_collection.clone(),
        instrument_or_sensor: winner.patch.instrument_or_sensor.clone(),
        assigned_modality,
        radiometric_class: winner.patch.radiometric_class,
        calibration_level: winner.patch.calibration_level,
        stacking_history: winner.patch.stacking_history.clone(),
        color_mapping: winner.patch.color_mapping.clone(),
        effective_verifiable_gsd_m: winner.effective_gsd_m,
        native_gsd_m: winner.patch.native_gsd_m,
        served_grid_spacing_m: winner.patch.served_grid_spacing_m,
        was_upsampled_beyond_native: winner.patch.was_upsampled_beyond_native(),
        time_window: winner.patch.time_window,
        capture_midpoint_utc_ms: winner.capture_midpoint_utc_ms,
        registration: winner.patch.registration.clone(),
        horizontal_rmse_m: winner.patch.registration.horizontal_rmse_m,
        off_nadir_angle_deg: winner.patch.registration.off_nadir_angle_deg,
        cell_integrity: winner.primary_cell_integrity.clone(),
        applicable_cell_states: winner.applicable_cell_states.clone(),
        bypassed_degraded_patch_ids,
    };

    let outcome = match winner.tier {
        CandidateTier::CleanOptical => CompositeCellOutcome::AssignedCleanOptical(assignment),
        CandidateTier::RadarFallback => CompositeCellOutcome::AssignedRadarFallback(assignment),
        CandidateTier::DegradedOpticalFallback => {
            let degradation_warning = match &winner.primary_cell_integrity {
                PatchIntegrityState::DeliberatelyBlurredOrRedacted {
                    reason,
                    coarse_effective_gsd_m: Some(coarse_gsd),
                } => format!(
                    "No unredacted source available; retained blurred/redacted patch '{}' ({reason}, effective GSD {coarse_gsd:.2} m)",
                    winner.patch.patch_id
                ),
                PatchIntegrityState::DeliberatelyBlurredOrRedacted {
                    reason,
                    coarse_effective_gsd_m: None,
                } => format!(
                    "No unredacted source available; retained blurred/redacted patch '{}' ({reason}, coarse GSD unknown)",
                    winner.patch.patch_id
                ),
                PatchIntegrityState::ObscuredByCloudShadowOrSmoke {
                    obscuration_fraction,
                } => format!(
                    "No cloud-free source available; retained obscured patch '{}' ({:.1}% obscured)",
                    winner.patch.patch_id,
                    obscuration_fraction * 100.0
                ),
                PatchIntegrityState::UneditedClearNative {
                    cloud_or_haze_fraction,
                } => format!(
                    "No low-cloud source available; retained hazy patch '{}' ({:.1}% cloud/haze)",
                    winner.patch.patch_id,
                    cloud_or_haze_fraction * 100.0
                ),
                _ => format!(
                    "Retained degraded fallback patch '{}'",
                    winner.patch.patch_id
                ),
            };
            CompositeCellOutcome::AssignedDegradedOpticalFallback {
                assignment,
                degradation_warning,
            }
        }
    };

    Ok(CompositeCellPlan {
        row,
        col,
        bounds: cell_bounds,
        outcome,
        rejected_or_bypassed,
        omitted_rejected_receipts_count,
    })
}

fn compare_optional_gsd_total(a: Option<f64>, b: Option<f64>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(av), Some(bv)) => av.total_cmp(&bv),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn compare_evaluated_candidates(
    a: &EvaluatedCellCandidate<'_>,
    b: &EvaluatedCellCandidate<'_>,
) -> std::cmp::Ordering {
    // Strict total-order comparator (AGENTS.md §5): uses f64::total_cmp without non-transitive
    // epsilon bands, followed by a deterministic tie-break on patch_id.
    a.tier
        .cmp(&b.tier)
        .then_with(|| compare_optional_gsd_total(a.effective_gsd_m, b.effective_gsd_m))
        .then_with(|| {
            a.patch
                .was_upsampled_beyond_native()
                .cmp(&b.patch.was_upsampled_beyond_native())
        })
        .then_with(|| {
            b.patch
                .radiometric_class
                .is_true_color_optical()
                .cmp(&a.patch.radiometric_class.is_true_color_optical())
        })
        .then_with(|| a.obscuration_metric.total_cmp(&b.obscuration_metric))
        .then_with(|| {
            a.patch
                .registration
                .horizontal_rmse_m
                .total_cmp(&b.patch.registration.horizontal_rmse_m)
        })
        .then_with(|| {
            a.patch
                .registration
                .off_nadir_angle_deg
                .total_cmp(&b.patch.registration.off_nadir_angle_deg)
        })
        .then_with(|| a.patch.patch_id.cmp(&b.patch.patch_id))
}

fn compute_window_midpoint_ms(window: &TimeWindow) -> i64 {
    let start = i128::from(window.start_utc_ms());
    let end = i128::from(window.end_utc_ms());
    let mid = start + (end - start) / 2;
    mid.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

fn compute_grid_cell_bounds(
    envelope: &GeoBoundingBoxWgs84,
    row: usize,
    col: usize,
    grid_rows: usize,
    grid_cols: usize,
) -> Result<GeoBoundingBoxWgs84, SpaceImageryError> {
    let south = envelope.south_lat_deg();
    let north = envelope.north_lat_deg();
    let lat_span = north - south;

    let cell_south = south + lat_span * (row as f64) / (grid_rows as f64);
    let cell_north = if row + 1 == grid_rows {
        north
    } else {
        south + lat_span * ((row + 1) as f64) / (grid_rows as f64)
    };

    let west = envelope.west_lon_deg();
    let east = envelope.east_lon_deg();
    let (cell_west, cell_east) = if west <= east {
        let lon_span = east - west;
        let cw = west + lon_span * (col as f64) / (grid_cols as f64);
        let ce = if col + 1 == grid_cols {
            east
        } else {
            west + lon_span * ((col + 1) as f64) / (grid_cols as f64)
        };
        (cw, ce)
    } else {
        let lon_span = (180.0 - west) + (east - (-180.0));
        let raw_w = west + lon_span * (col as f64) / (grid_cols as f64);
        let raw_e = west + lon_span * ((col + 1) as f64) / (grid_cols as f64);
        if raw_e <= 180.0 + 1e-12 {
            (raw_w.min(180.0), raw_e.min(180.0))
        } else if raw_w >= 180.0 - 1e-12 {
            let cw = (raw_w - 360.0).max(-180.0);
            let ce = if col + 1 == grid_cols {
                east
            } else {
                (raw_e - 360.0).min(east)
            };
            (cw, ce)
        } else {
            let cw = raw_w;
            let ce = if col + 1 == grid_cols {
                east
            } else {
                raw_e - 360.0
            };
            (cw, ce)
        }
    };

    GeoBoundingBoxWgs84::new(cell_south, cell_north, cell_west, cell_east).map_err(|_| {
        SpaceImageryError::InvalidCoordinates {
            reason: "failed to construct composite grid cell bounding box",
        }
    })
}

fn is_full_longitude_span(envelope: &GeoBoundingBoxWgs84) -> bool {
    let lon_span = if envelope.west_lon_deg() <= envelope.east_lon_deg() {
        envelope.east_lon_deg() - envelope.west_lon_deg()
    } else {
        (180.0 - envelope.west_lon_deg()) + (envelope.east_lon_deg() - (-180.0))
    };
    (lon_span - 360.0).abs() <= 1e-7
}

fn compute_composite_seams(
    cells: &[CompositeCellPlan],
    grid_rows: usize,
    grid_cols: usize,
    wraps_full_globe_lon: bool,
) -> Result<Vec<CompositeSeamSegment>, SpaceImageryError> {
    let mut seams = Vec::new();

    for row in 0..grid_rows {
        for col in 0..grid_cols {
            let idx = row * grid_cols + col;
            let Some(first) = cells[idx].outcome.assignment() else {
                continue;
            };

            // Horizontal neighbor (row, col + 1)
            if col + 1 < grid_cols {
                let right_idx = row * grid_cols + (col + 1);
                if let Some(second) = cells[right_idx].outcome.assignment()
                    && first.patch_id != second.patch_id
                {
                    seams.push(build_seam_segment(row, col, row, col + 1, first, second)?);
                }
            } else if wraps_full_globe_lon && grid_cols > 1 {
                // Full-globe longitude wraparound seam between (row, grid_cols - 1) and (row, 0)
                let wrap_idx = row * grid_cols;
                if let Some(second) = cells[wrap_idx].outcome.assignment()
                    && first.patch_id != second.patch_id
                {
                    seams.push(build_seam_segment(row, col, row, 0, first, second)?);
                }
            }

            // Vertical neighbor (row + 1, col)
            if row + 1 < grid_rows {
                let down_idx = (row + 1) * grid_cols + col;
                if let Some(second) = cells[down_idx].outcome.assignment()
                    && first.patch_id != second.patch_id
                {
                    seams.push(build_seam_segment(row, col, row + 1, col, first, second)?);
                }
            }
        }
    }

    Ok(seams)
}

fn build_seam_segment(
    first_row: usize,
    first_col: usize,
    second_row: usize,
    second_col: usize,
    first: &CompositeCellAssignment,
    second: &CompositeCellAssignment,
) -> Result<CompositeSeamSegment, SpaceImageryError> {
    let delta_i128 = (i128::from(first.capture_midpoint_utc_ms)
        - i128::from(second.capture_midpoint_utc_ms))
    .abs();
    let capture_time_delta_ms =
        i64::try_from(delta_i128).map_err(|_| SpaceImageryError::TimestampOverflow {
            operation: "seam_capture_time_delta_ms",
            reason: "capture timestamp difference between adjacent composite patches overflowed i64",
        })?;

    let effective_gsd_ratio = match (
        first.effective_verifiable_gsd_m,
        second.effective_verifiable_gsd_m,
    ) {
        (Some(g1), Some(g2)) => {
            let min_gsd = g1.min(g2);
            let max_gsd = g1.max(g2);
            if min_gsd > 0.0 {
                Some(max_gsd / min_gsd)
            } else {
                None
            }
        }
        _ => None,
    };

    Ok(CompositeSeamSegment {
        first_cell_row: first_row,
        first_cell_col: first_col,
        second_cell_row: second_row,
        second_cell_col: second_col,
        first_patch_id: first.patch_id.clone(),
        second_patch_id: second.patch_id.clone(),
        capture_time_delta_ms,
        effective_gsd_ratio,
        crosses_sensor_modality: first.assigned_modality != second.assigned_modality,
        crosses_platform_class: first.platform != second.platform,
    })
}

/// Decomposes a longitude interval into at most two non-wrapped intervals `[w, e]` where `w <= e`.
fn decompose_lon_intervals(west: f64, east: f64) -> Vec<(f64, f64)> {
    if west <= east {
        vec![(west, east)]
    } else {
        vec![(west, 180.0), (-180.0, east)]
    }
}

/// Computes the exact geometric spatial intersection components of two WGS84 bounding boxes,
/// preserving all disjoint components and correctly supporting antimeridian dateline crossing.
fn compute_box_intersections(
    b1: &GeoBoundingBoxWgs84,
    b2: &GeoBoundingBoxWgs84,
) -> Vec<GeoBoundingBoxWgs84> {
    let s = b1.south_lat_deg().max(b2.south_lat_deg());
    let n = b1.north_lat_deg().min(b2.north_lat_deg());
    if s >= n {
        return Vec::new();
    }

    let l1 = decompose_lon_intervals(b1.west_lon_deg(), b1.east_lon_deg());
    let l2 = decompose_lon_intervals(b2.west_lon_deg(), b2.east_lon_deg());

    let mut raw_intervals = Vec::new();
    for (w1, e1) in &l1 {
        for (w2, e2) in &l2 {
            let w = w1.max(*w2);
            let e = e1.min(*e2);
            if w < e {
                raw_intervals.push((w, e));
            }
        }
    }

    if raw_intervals.is_empty() {
        return Vec::new();
    }

    // Check if intervals wrap across antimeridian: one ends at 180.0 and one starts at -180.0
    let pos_piece_idx = raw_intervals
        .iter()
        .position(|(_w, e)| (*e - 180.0).abs() < 1e-7);
    let neg_piece_idx = raw_intervals
        .iter()
        .position(|(w, _e)| (*w - (-180.0)).abs() < 1e-7);

    let mut result = Vec::new();

    if let (Some(pi), Some(ni)) = (pos_piece_idx, neg_piece_idx)
        && pi != ni
    {
        let pos_w = raw_intervals[pi].0;
        let neg_e = raw_intervals[ni].1;
        if let Ok(wrapped_box) = GeoBoundingBoxWgs84::new(s, n, pos_w, neg_e) {
            result.push(wrapped_box);
        }
        for (i, &(w, e)) in raw_intervals.iter().enumerate() {
            if i != pi
                && i != ni
                && let Ok(b) = GeoBoundingBoxWgs84::new(s, n, w, e)
            {
                result.push(b);
            }
        }
    } else {
        for (w, e) in raw_intervals {
            if let Ok(b) = GeoBoundingBoxWgs84::new(s, n, w, e) {
                result.push(b);
            }
        }
    }

    result
}

/// Evaluates envelope rectangle union coverage using latitude band slicing across
/// decomposed non-wrapped longitude intervals.
fn evaluate_envelope_union_coverage(
    envelope: &GeoBoundingBoxWgs84,
    footprints: &[ObservationFootprint],
) -> (f64, usize) {
    let env_south = envelope.south_lat_deg();
    let env_north = envelope.north_lat_deg();
    let total_lat_span = env_north - env_south;
    if total_lat_span <= 0.0 || footprints.is_empty() {
        return (1.0, 1);
    }

    // Decompose envelope into non-wrapped longitude pieces
    let env_lon_pieces = decompose_lon_intervals(envelope.west_lon_deg(), envelope.east_lon_deg());

    // Decompose each footprint into non-wrapped longitude pieces clipped to envelope latitude
    struct FootprintPiece {
        south: f64,
        north: f64,
        west: f64,
        east: f64,
    }

    let mut footprint_pieces = Vec::new();
    for f in footprints {
        let fs = f.bounding_box.south_lat_deg().max(env_south);
        let fn_ = f.bounding_box.north_lat_deg().min(env_north);
        if fs >= fn_ {
            continue;
        }
        for (fw, fe) in
            decompose_lon_intervals(f.bounding_box.west_lon_deg(), f.bounding_box.east_lon_deg())
        {
            footprint_pieces.push(FootprintPiece {
                south: fs,
                north: fn_,
                west: fw,
                east: fe,
            });
        }
    }

    let mut total_envelope_area = 0.0;
    let mut total_covered_area = 0.0;
    let mut total_uncovered_bands = 0;

    for (target_w, target_e) in env_lon_pieces {
        let piece_lon_span = target_e - target_w;
        if piece_lon_span <= 0.0 {
            continue;
        }
        let piece_envelope_area = total_lat_span * piece_lon_span;
        total_envelope_area += piece_envelope_area;

        // Collect relevant footprint pieces clipped to this target piece
        let mut relevant_pieces = Vec::new();
        let mut lat_cuts = vec![env_south, env_north];

        for fp in &footprint_pieces {
            let cw = fp.west.max(target_w);
            let ce = fp.east.min(target_e);
            if cw < ce {
                relevant_pieces.push(FootprintPiece {
                    south: fp.south,
                    north: fp.north,
                    west: cw,
                    east: ce,
                });
                lat_cuts.push(fp.south);
                lat_cuts.push(fp.north);
            }
        }

        lat_cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        lat_cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-7);

        for i in 0..lat_cuts.len().saturating_sub(1) {
            let y0 = lat_cuts[i];
            let y1 = lat_cuts[i + 1];
            let dy = y1 - y0;
            if dy <= 1e-9 {
                continue;
            }

            let mut intervals = Vec::new();
            for rp in &relevant_pieces {
                if rp.south <= y0 + 1e-9 && rp.north >= y1 - 1e-9 {
                    intervals.push((rp.west, rp.east));
                }
            }

            if intervals.is_empty() {
                total_uncovered_bands += 1;
                continue;
            }

            intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let mut merged: Vec<(f64, f64)> = Vec::new();
            for (w, e) in intervals {
                if let Some(last) = merged.last_mut() {
                    if w <= last.1 + 1e-9 {
                        if e > last.1 {
                            last.1 = e;
                        }
                    } else {
                        merged.push((w, e));
                    }
                } else {
                    merged.push((w, e));
                }
            }

            let slice_covered_lon: f64 = merged.iter().map(|(w, e)| e - w).sum();
            total_covered_area += dy * slice_covered_lon;

            if slice_covered_lon < piece_lon_span - 1e-6 {
                total_uncovered_bands += 1;
            }
        }
    }

    if total_envelope_area <= 0.0 {
        return (1.0, 1);
    }

    let uncovered_fraction =
        ((total_envelope_area - total_covered_area) / total_envelope_area).clamp(0.0, 1.0);
    (uncovered_fraction, total_uncovered_bands)
}

fn validate_str(field: &'static str, s: &str) -> Result<(), SpaceImageryError> {
    if s.trim().is_empty() {
        return Err(SpaceImageryError::EmptyIdentifier { field });
    }
    if s.len() > MAX_STRING_LEN {
        return Err(SpaceImageryError::StringTooLong {
            field,
            max: MAX_STRING_LEN,
            actual: s.len(),
        });
    }
    Ok(())
}
