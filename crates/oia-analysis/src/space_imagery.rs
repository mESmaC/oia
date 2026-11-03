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

use std::error::Error;
use std::fmt;

use crate::geo_context::provisional::{GeoBoundingBoxWgs84, GeoPointWgs84, TimeWindow};
pub use crate::multilingual::StopReason;

/// Method identifier for space imagery catalog and observation analysis.
pub const METHOD_ID: &str = "oia-space-imagery-catalog-v1";

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
    ObservationNotFound {
        id: String,
    },
    IncompatibleBaselineComparison {
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
            Self::ObservationNotFound { id } => {
                write!(f, "space observation ID not found: '{id}'")
            }
            Self::IncompatibleBaselineComparison { reason } => {
                write!(f, "incompatible baseline comparison: {reason}")
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
