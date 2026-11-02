//! Reproducible investigation bundle validation, staged SHA-256 verification, local namespace
//! mapping, and deterministic/tolerance-based analysis replay (`#39`, `#22`, `REPRO-01`–`REPRO-03`,
//! `A17`, `ADR 0006` / `D05`, `docs/contracts/report-export.md` §§5–6).
//!
//! This module is standard-library-only and enforces explicit finite budgets, cooperative
//! cancellation/deadline checks, relative path and container safety, FIPS 180-4 SHA-256 staged
//! integrity verification, local workspace namespace isolation, and explicit replay evaluation.
//! Opening or validating a bundle never executes code, connects devices, fetches remote assets,
//! or triggers analysis replay automatically.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use crate::{OmissionReason, OriginClass, ReviewStatus, StopReason, TimeDisplay, TimePrecision};

/// Canonical bundle format identifier (`docs/contracts/report-export.md` §5).
pub const SUPPORTED_BUNDLE_FORMAT: &str = "oia-investigation-bundle";

/// Canonical bundle format schema version supported by this kernel.
pub const SUPPORTED_BUNDLE_FORMAT_VERSION: u32 = 1;

/// Resource budget dimensions for bundle staging, validation, and replay (`REPRO-02`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BundleLimitKind {
    Capabilities,
    Entries,
    Records,
    References,
    Omissions,
    ReproductionItems,
    FileBytes,
    TotalBytes,
    TextBytes,
    ReplaySamples,
}

/// Configured finite resource limits for bundle import/export validation and replay
/// (`docs/contracts/report-export.md` §6).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundleLimits {
    max_capabilities: usize,
    max_entries: usize,
    max_records: usize,
    max_references: usize,
    max_omissions: usize,
    max_reproduction_items: usize,
    max_file_bytes: usize,
    max_total_bytes: usize,
    max_text_bytes: usize,
    max_replay_samples: usize,
}

impl BundleLimits {
    /// Default conservative in-memory validation limits. These defaults are bounded operational
    /// guards, not measured hardware ceilings (`docs/contracts/report-export.md` §6).
    pub const DEFAULT_MAX_CAPABILITIES: usize = 64;
    pub const DEFAULT_MAX_ENTRIES: usize = 256;
    pub const DEFAULT_MAX_RECORDS: usize = 512;
    pub const DEFAULT_MAX_REFERENCES: usize = 1024;
    pub const DEFAULT_MAX_OMISSIONS: usize = 256;
    pub const DEFAULT_MAX_REPRODUCTION_ITEMS: usize = 256;
    pub const DEFAULT_MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
    pub const DEFAULT_MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
    pub const DEFAULT_MAX_TEXT_BYTES: usize = 1024 * 1024;
    pub const DEFAULT_MAX_REPLAY_SAMPLES: usize = 16_384;

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        max_capabilities: usize,
        max_entries: usize,
        max_records: usize,
        max_references: usize,
        max_omissions: usize,
        max_reproduction_items: usize,
        max_file_bytes: usize,
        max_total_bytes: usize,
        max_text_bytes: usize,
        max_replay_samples: usize,
    ) -> Result<Self, BundleError> {
        if max_capabilities == 0 {
            return Err(BundleError::ZeroLimit(BundleLimitKind::Capabilities));
        }
        if max_entries == 0 {
            return Err(BundleError::ZeroLimit(BundleLimitKind::Entries));
        }
        if max_records == 0 {
            return Err(BundleError::ZeroLimit(BundleLimitKind::Records));
        }
        if max_references == 0 {
            return Err(BundleError::ZeroLimit(BundleLimitKind::References));
        }
        if max_omissions == 0 {
            return Err(BundleError::ZeroLimit(BundleLimitKind::Omissions));
        }
        if max_reproduction_items == 0 {
            return Err(BundleError::ZeroLimit(BundleLimitKind::ReproductionItems));
        }
        if max_file_bytes == 0 {
            return Err(BundleError::ZeroLimit(BundleLimitKind::FileBytes));
        }
        if max_total_bytes == 0 {
            return Err(BundleError::ZeroLimit(BundleLimitKind::TotalBytes));
        }
        if max_text_bytes == 0 {
            return Err(BundleError::ZeroLimit(BundleLimitKind::TextBytes));
        }
        if max_replay_samples == 0 {
            return Err(BundleError::ZeroLimit(BundleLimitKind::ReplaySamples));
        }
        Ok(Self {
            max_capabilities,
            max_entries,
            max_records,
            max_references,
            max_omissions,
            max_reproduction_items,
            max_file_bytes,
            max_total_bytes,
            max_text_bytes,
            max_replay_samples,
        })
    }

    pub fn default_limits() -> Self {
        Self {
            max_capabilities: Self::DEFAULT_MAX_CAPABILITIES,
            max_entries: Self::DEFAULT_MAX_ENTRIES,
            max_records: Self::DEFAULT_MAX_RECORDS,
            max_references: Self::DEFAULT_MAX_REFERENCES,
            max_omissions: Self::DEFAULT_MAX_OMISSIONS,
            max_reproduction_items: Self::DEFAULT_MAX_REPRODUCTION_ITEMS,
            max_file_bytes: Self::DEFAULT_MAX_FILE_BYTES,
            max_total_bytes: Self::DEFAULT_MAX_TOTAL_BYTES,
            max_text_bytes: Self::DEFAULT_MAX_TEXT_BYTES,
            max_replay_samples: Self::DEFAULT_MAX_REPLAY_SAMPLES,
        }
    }

    pub fn max_capabilities(self) -> usize {
        self.max_capabilities
    }

    pub fn max_entries(self) -> usize {
        self.max_entries
    }

    pub fn max_records(self) -> usize {
        self.max_records
    }

    pub fn max_references(self) -> usize {
        self.max_references
    }

    pub fn max_omissions(self) -> usize {
        self.max_omissions
    }

    pub fn max_reproduction_items(self) -> usize {
        self.max_reproduction_items
    }

    pub fn max_file_bytes(self) -> usize {
        self.max_file_bytes
    }

    pub fn max_total_bytes(self) -> usize {
        self.max_total_bytes
    }

    pub fn max_text_bytes(self) -> usize {
        self.max_text_bytes
    }

    pub fn max_replay_samples(self) -> usize {
        self.max_replay_samples
    }
}

/// Structured errors returned during bundle staging, validation, namespace mapping, and replay
/// (`docs/contracts/report-export.md` §§5–6, `REPRO-01`–`REPRO-03`, `A17`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BundleError {
    ZeroLimit(BundleLimitKind),
    LimitExceeded {
        limit: BundleLimitKind,
        actual: usize,
        max: usize,
    },
    UnsupportedBundleFormat {
        format: String,
        version: u32,
    },
    UnsupportedRequiredCapability {
        capability_id: String,
        version: u32,
    },
    EmptyField {
        field: &'static str,
    },
    InvalidTextEncoding {
        field: &'static str,
        reason: &'static str,
    },
    InvalidTimeDisplay {
        field: &'static str,
        reason: &'static str,
    },
    DuplicateIdentifier {
        kind: &'static str,
        id: String,
    },
    InvalidRelativePath {
        path: String,
        reason: &'static str,
    },
    CaseInsensitivePathCollision {
        first_path: String,
        colliding_path: String,
    },
    UnsafeFilesystemEntry {
        path: String,
        entry_kind: StagedFilesystemEntryKind,
    },
    ArchiveContainerRejected {
        path: String,
        container_kind: &'static str,
    },
    InvalidSha256Hex {
        context: &'static str,
        value: String,
    },
    AssetFilenameHashMismatch {
        path: String,
        declared_sha256_hex: String,
    },
    MissingStagedPayload {
        path: String,
    },
    UnlistedStagedPayload {
        path: String,
    },
    ByteLengthMismatch {
        path: String,
        declared_len: usize,
        actual_len: usize,
    },
    IntegrityHashMismatch {
        path: String,
        declared_sha256_hex: String,
        actual_sha256_hex: String,
    },
    InputChangedDuringStaging {
        path: String,
        initial_sha256_hex: String,
        rechecked_sha256_hex: String,
    },
    UnexplainedDanglingReference {
        from_id: String,
        target_id: String,
    },
    InvalidRecordSchema {
        record_id: String,
        reason: &'static str,
    },
    ExistingCaseOverwriteRefused {
        destination_case_id: String,
    },
    ForbiddenBundleSideEffect {
        context: String,
        reason: &'static str,
    },
    InvalidToleranceConfiguration {
        analysis_id: String,
        reason: &'static str,
    },
    InvalidReplayInput {
        analysis_id: String,
        reason: &'static str,
    },
    UnknownAnalysisRun {
        analysis_id: String,
        version_id: String,
    },
    AllocationFailed,
    Stopped(StopReason),
}

impl fmt::Display for BundleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit(limit) => {
                write!(formatter, "{limit:?} limit must be greater than zero")
            }
            Self::LimitExceeded { limit, actual, max } => write!(
                formatter,
                "{limit:?} budget exceeded: actual {actual} exceeds limit {max}"
            ),
            Self::UnsupportedBundleFormat { format, version } => write!(
                formatter,
                "unsupported bundle format `{format}` version {version} (expected `{SUPPORTED_BUNDLE_FORMAT}` v{SUPPORTED_BUNDLE_FORMAT_VERSION})"
            ),
            Self::UnsupportedRequiredCapability {
                capability_id,
                version,
            } => write!(
                formatter,
                "bundle requires unsupported capability `{capability_id}` v{version}"
            ),
            Self::EmptyField { field } => {
                write!(formatter, "required field `{field}` must not be empty")
            }
            Self::InvalidTextEncoding { field, reason } => {
                write!(formatter, "invalid text in `{field}`: {reason}")
            }
            Self::InvalidTimeDisplay { field, reason } => {
                write!(formatter, "invalid time display in `{field}`: {reason}")
            }
            Self::DuplicateIdentifier { kind, id } => {
                write!(formatter, "duplicate {kind} identifier `{id}`")
            }
            Self::InvalidRelativePath { path, reason } => {
                write!(formatter, "invalid bundle relative path `{path}`: {reason}")
            }
            Self::CaseInsensitivePathCollision {
                first_path,
                colliding_path,
            } => write!(
                formatter,
                "bundle paths `{first_path}` and `{colliding_path}` collide under case-insensitive platform rules"
            ),
            Self::UnsafeFilesystemEntry { path, entry_kind } => write!(
                formatter,
                "bundle entry `{path}` uses non-regular filesystem kind `{entry_kind:?}`"
            ),
            Self::ArchiveContainerRejected {
                path,
                container_kind,
            } => write!(
                formatter,
                "archive or compressed container `{path}` ({container_kind}) is rejected without unpacking"
            ),
            Self::InvalidSha256Hex { context, value } => write!(
                formatter,
                "invalid 64-character lowercase hex SHA-256 digest in {context}: `{value}`"
            ),
            Self::AssetFilenameHashMismatch {
                path,
                declared_sha256_hex,
            } => write!(
                formatter,
                "asset path `{path}` does not match declared SHA-256 `{declared_sha256_hex}`"
            ),
            Self::MissingStagedPayload { path } => write!(
                formatter,
                "manifest entry `{path}` has no corresponding staged payload"
            ),
            Self::UnlistedStagedPayload { path } => write!(
                formatter,
                "staged payload `{path}` is not declared in manifest entry inventory"
            ),
            Self::ByteLengthMismatch {
                path,
                declared_len,
                actual_len,
            } => write!(
                formatter,
                "staged payload `{path}` length {actual_len} does not match declared length {declared_len}"
            ),
            Self::IntegrityHashMismatch {
                path,
                declared_sha256_hex,
                actual_sha256_hex,
            } => write!(
                formatter,
                "staged payload `{path}` SHA-256 `{actual_sha256_hex}` does not match declared `{declared_sha256_hex}`"
            ),
            Self::InputChangedDuringStaging {
                path,
                initial_sha256_hex,
                rechecked_sha256_hex,
            } => write!(
                formatter,
                "staged payload `{path}` changed during staging (`{initial_sha256_hex}` -> `{rechecked_sha256_hex}`)"
            ),
            Self::UnexplainedDanglingReference { from_id, target_id } => write!(
                formatter,
                "reference from `{from_id}` to `{target_id}` does not resolve to an included or declared unavailable target"
            ),
            Self::InvalidRecordSchema { record_id, reason } => {
                write!(formatter, "invalid record `{record_id}`: {reason}")
            }
            Self::ExistingCaseOverwriteRefused {
                destination_case_id,
            } => write!(
                formatter,
                "refusing to overwrite existing destination case `{destination_case_id}`"
            ),
            Self::ForbiddenBundleSideEffect { context, reason } => write!(
                formatter,
                "forbidden executable, network, or device side effect in `{context}`: {reason}"
            ),
            Self::InvalidToleranceConfiguration {
                analysis_id,
                reason,
            } => write!(
                formatter,
                "invalid numerical tolerance on analysis `{analysis_id}`: {reason}"
            ),
            Self::InvalidReplayInput {
                analysis_id,
                reason,
            } => write!(
                formatter,
                "invalid replay input for analysis `{analysis_id}`: {reason}"
            ),
            Self::UnknownAnalysisRun {
                analysis_id,
                version_id,
            } => write!(
                formatter,
                "analysis `{analysis_id}` version `{version_id}` is not present in the validated bundle"
            ),
            Self::AllocationFailed => write!(formatter, "fallible memory allocation failed"),
            Self::Stopped(reason) => write!(formatter, "bundle operation stopped: {reason:?}"),
        }
    }
}

impl Error for BundleError {}

/// Package identity metadata declared in `manifest.json` (`docs/contracts/report-export.md` §5).
/// Package identity is provenance metadata, never a trusted authority grant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageIdentity<'input> {
    pub format: &'input str,
    pub format_version: u32,
    pub package_id: &'input str,
    pub created_time: TimeDisplay<'input>,
    pub producer_id: &'input str,
    pub producer_version: &'input str,
}

/// Capability category declared by a bundle (`docs/contracts/report-export.md` §5).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CapabilityKind {
    RecordSchema,
    AnchorSpace,
    AnalysisMethod,
    ForecastSchema,
    CalibrationModel,
}

/// Whether a capability is required to import the bundle or optional.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityRequirement {
    Required,
    Optional,
}

/// Single capability requirement in the bundle manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityDeclaration<'input> {
    pub capability_id: &'input str,
    pub kind: CapabilityKind,
    pub version: u32,
    pub requirement: CapabilityRequirement,
}

/// Capabilities supported by the importing workspace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportedCapabilities<'input> {
    pub supported: &'input [(&'input str, u32)],
    pub allow_inert_optional_records: bool,
}

/// Logical directory role for an entry in the portable directory bundle
/// (`docs/contracts/report-export.md` §5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BundleEntryRole {
    Records,
    Assets,
    Reports,
    Reproduction,
}

impl BundleEntryRole {
    pub fn required_prefix(self) -> &'static str {
        match self {
            Self::Records => "records/",
            Self::Assets => "assets/",
            Self::Reports => "reports/",
            Self::Reproduction => "reproduction/",
        }
    }
}

/// Declared entry in the bundle's `manifest.json` inventory (`docs/contracts/report-export.md` §5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundleManifestEntry<'input> {
    pub relative_path: &'input str,
    pub role: BundleEntryRole,
    pub media_type: &'input str,
    pub declared_byte_len: usize,
    pub declared_sha256_hex: &'input str,
    pub schema_version: Option<u32>,
}

/// Filesystem entry classification observed while staging (`docs/contracts/report-export.md` §6).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StagedFilesystemEntryKind {
    RegularFile,
    Symlink,
    Junction,
    ReparsePoint,
    DeviceNode,
    FifoOrSocket,
}

/// Staged file payload supplied to the validator after handle-relative/no-follow reading into a
/// private staging area (`docs/contracts/report-export.md` §6).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StagedPayloadFile<'input> {
    pub relative_path: &'input str,
    pub entry_kind: StagedFilesystemEntryKind,
    pub staged_bytes: &'input [u8],
    /// Optional SHA-256 hex observed on a second verification pass over the staged bytes.
    /// If present and different from the initial staged digest, import fails with
    /// `BundleError::InputChangedDuringStaging`.
    pub rechecked_sha256_hex: Option<&'input str>,
}

/// Retention permission for a bundled asset (`docs/contracts/report-export.md` §5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetentionCondition {
    Permitted,
    ReviewWindowOnly { window_note: &'static str },
    Prohibited,
}

/// Redistribution permission for a bundled asset (`docs/contracts/report-export.md` §5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RedistributionCondition {
    PermittedWithAttribution,
    PrivateCaseTransferOnly,
    ReferenceOnly,
    Prohibited,
}

/// Per-asset permission, attribution, retention, and redistribution conditions, preserved
/// separately from the OIA software license (`docs/contracts/report-export.md` §5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetUseConditions<'input> {
    pub asset_path: &'input str,
    pub source_id: &'input str,
    pub ordinary_access_method: &'input str,
    pub attribution_notice: &'input str,
    pub automated_processing_permitted: bool,
    pub retention: RetentionCondition,
    pub redistribution: RedistributionCondition,
}

/// Domain record category represented in the bundle's record inventory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BundleRecordKind {
    SourceDocument,
    EvidenceItem,
    Claim,
    Annotation,
    GroupingDecision,
    QueryDefinition,
    CalibrationRecord,
    AnalysisRun,
    ForecastRecord,
    ReportSnapshot,
}

/// Versioned record metadata in the bundle's record inventory (`REPRO-01`, `REPRO-02`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundleRecordEntry<'input> {
    pub origin_namespace: &'input str,
    pub record_id: &'input str,
    pub version_id: &'input str,
    pub kind: BundleRecordKind,
    pub origin_class: OriginClass,
    pub review_status: ReviewStatus,
    pub origin_actor_id: &'input str,
    pub recorded_time: TimeDisplay<'input>,
    pub uncertainty_note: Option<&'input str>,
    pub asset_relative_path: Option<&'input str>,
    pub required_capability_id: Option<&'input str>,
}

/// Resolution target for a reference inside the bundle (`docs/contracts/report-export.md` §5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BundleReferenceTarget<'input> {
    IncludedRecord {
        target_record_id: &'input str,
        target_version_id: &'input str,
    },
    ExternalReferenceOnly {
        target_record_id: &'input str,
        target_version_id: &'input str,
        source_id: &'input str,
        external_url: Option<&'input str>,
        reason: OmissionReason,
    },
    PermittedUnavailable {
        target_record_id: &'input str,
        target_version_id: &'input str,
        reason: OmissionReason,
        coverage_note: &'input str,
    },
}

/// Directed reference between versioned records in the bundle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundleReference<'input> {
    pub reference_id: &'input str,
    pub from_record_id: &'input str,
    pub from_version_id: &'input str,
    pub relation: &'input str,
    pub target: BundleReferenceTarget<'input>,
}

/// Explicit record of an omitted, restricted, or missing asset and its impact on coverage and
/// replay (`REPRO-01`, `docs/contracts/report-export.md` §§3, 5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundleOmissionEntry<'input> {
    pub omitted_id: &'input str,
    pub source_id: &'input str,
    pub version_id: &'input str,
    pub reason: OmissionReason,
    pub coverage_effect: &'input str,
    pub unreproducible_step_note: Option<&'input str>,
}

/// Pinned query specification in `reproduction/` (`REPRO-01`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PinnedQuerySpec<'input> {
    pub query_id: &'input str,
    pub version_id: &'input str,
    pub query_expression: &'input str,
    pub pinned_input_versions: &'input [(&'input str, &'input str)],
    pub coverage_note: &'input str,
}

/// Pinned grouping/clustering decision in `reproduction/` (`REPRO-01`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PinnedGroupingDecision<'input> {
    pub grouping_id: &'input str,
    pub version_id: &'input str,
    pub rationale: &'input str,
    pub member_record_versions: &'input [(&'input str, &'input str)],
    pub is_reversible: bool,
}

/// Validity status of a pinned camera or sensor calibration (`FIELD-03`, `REPRO-01`, `REPRO-03`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalibrationValidity<'input> {
    Valid,
    Invalidated { reason: &'input str },
}

/// Pinned camera/sensor/time calibration record in `reproduction/` (`REPRO-01`, `A17`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinnedCalibrationSpec<'input> {
    pub calibration_id: &'input str,
    pub version_id: &'input str,
    pub sensor_or_view_id: &'input str,
    pub reference_description: &'input str,
    pub residual_rms: f64,
    pub residual_unit: &'input str,
    pub validity: CalibrationValidity<'input>,
}

/// Pinned forecast specification in `reproduction/` (`REPRO-01`, `A17`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PinnedForecastSpec<'input> {
    pub forecast_id: &'input str,
    pub version_id: &'input str,
    pub cutoff_time: TimeDisplay<'input>,
    pub prediction_summary: &'input str,
    pub pinned_input_versions: &'input [(&'input str, &'input str)],
}

/// License compatibility state for an analysis method or model (`REPRO-03`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisLicenseStatus<'input> {
    Compatible { license_id: &'input str },
    ChangedOrIncompatible { detail: &'input str },
}

/// Replay determinism contract for a pinned analysis run (`REPRO-03`, `AGENTS.md` §5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReplayContract<'input> {
    /// Output bytes must match bit-for-bit via SHA-256.
    DeterministicBitwise {
        expected_output_sha256_hex: &'input str,
        expected_byte_len: usize,
    },
    /// Floating-point series must agree within justified absolute and relative tolerances.
    NumericalTolerance {
        expected_series: &'input [f64],
        max_abs_diff: f64,
        max_rel_diff: f64,
        tolerance_justification: &'input str,
    },
    /// Method is stochastic or non-deterministic and cannot claim exact or tolerance replay.
    NonReplayableStochastic { reason: &'input str },
}

/// Pinned analysis run specification in `reproduction/` (`REPRO-01`, `REPRO-03`, `A17`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinnedAnalysisSpec<'input> {
    pub analysis_id: &'input str,
    pub version_id: &'input str,
    pub method_id: &'input str,
    pub required_capability_id: &'input str,
    pub required_capability_version: u32,
    pub model_id: Option<&'input str>,
    pub model_version: Option<&'input str>,
    pub configuration_summary: &'input str,
    pub license_status: AnalysisLicenseStatus<'input>,
    pub pinned_input_versions: &'input [(&'input str, &'input str)],
    pub pinned_asset_paths: &'input [&'input str],
    pub calibration_ref: Option<(&'input str, &'input str)>,
    pub replay_contract: ReplayContract<'input>,
}

/// Complete bundle manifest and staged payload input for validation and staging (`REPRO-01`,
/// `REPRO-02`, `A17`).
#[derive(Clone, Debug, PartialEq)]
pub struct BundleImportInput<'input> {
    pub identity: PackageIdentity<'input>,
    pub destination_case_id: &'input str,
    pub local_namespace_prefix: &'input str,
    pub existing_case_ids: &'input [&'input str],
    pub capabilities: &'input [CapabilityDeclaration<'input>],
    pub manifest_entries: &'input [BundleManifestEntry<'input>],
    pub staged_files: &'input [StagedPayloadFile<'input>],
    pub use_conditions: &'input [AssetUseConditions<'input>],
    pub records: &'input [BundleRecordEntry<'input>],
    pub references: &'input [BundleReference<'input>],
    pub omissions: &'input [BundleOmissionEntry<'input>],
    pub queries: &'input [PinnedQuerySpec<'input>],
    pub groupings: &'input [PinnedGroupingDecision<'input>],
    pub calibrations: &'input [PinnedCalibrationSpec<'input>],
    pub forecasts: &'input [PinnedForecastSpec<'input>],
    pub analyses: &'input [PinnedAnalysisSpec<'input>],
}

/// Explicit guarantee that an imported actor identity confers zero local workspace permissions
/// (`ADR 0006`, `docs/contracts/report-export.md` §5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalAuthorityGrant {
    None,
}

/// Imported actor identity preserved in its origin namespace without local authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttributedOriginActor {
    pub origin_namespace: String,
    pub origin_actor_id: String,
    pub local_authority: LocalAuthorityGrant,
}

/// How a record depending on an unsupported optional capability was handled during import
/// (`docs/contracts/report-export.md` §6).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptionalRecordHandling {
    Supported,
    RetainedInertOpaqueBytes { capability_id: String },
    OmittedWithNotice { capability_id: String },
}

/// How bundled readable reports under `reports/` are treated on import (`REPRO-02`,
/// `docs/contracts/report-export.md` §6).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImportedReportHandling {
    UntrustedRequiresRegenerationOrInertViewer,
}

/// Validated staged file entry with verified SHA-256 and byte length.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedStagedEntry {
    pub relative_path: String,
    pub role: BundleEntryRole,
    pub media_type: String,
    pub verified_byte_len: usize,
    pub verified_sha256_hex: String,
    pub report_handling: Option<ImportedReportHandling>,
}

/// Mapped record in the destination workspace's local namespace (`docs/contracts/report-export.md`
/// §§5–6).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MappedBundleRecord {
    pub origin_namespace: String,
    pub origin_record_id: String,
    pub origin_version_id: String,
    pub local_record_id: String,
    pub local_version_id: String,
    pub kind: BundleRecordKind,
    pub origin_class: OriginClass,
    pub review_status: ReviewStatus,
    pub attributed_actor: AttributedOriginActor,
    pub asset_sha256_hex: Option<String>,
    pub optional_handling: OptionalRecordHandling,
}

/// Validated, staged, and namespace-mapped bundle ready for atomic storage commit or explicit
/// analysis replay (`REPRO-01`–`REPRO-03`, `A17`).
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedBundle {
    package_id: String,
    destination_case_id: String,
    local_namespace_prefix: String,
    verified_entries: Vec<VerifiedStagedEntry>,
    mapped_records: Vec<MappedBundleRecord>,
    omitted_optional_capabilities: Vec<String>,
    omission_count: usize,
    /// Guarantee that validation performed zero code execution, zero network fetches, zero device
    /// connections, and zero automatic analysis replays.
    inert_import_verified: bool,
    calibrations: BTreeMap<(String, String), ValidatedCalibration>,
    available_record_versions: BTreeSet<(String, String)>,
    omitted_record_versions: BTreeMap<(String, String), OmissionReason>,
    verified_asset_digests: BTreeMap<String, String>,
    supported_capabilities: BTreeSet<(String, u32)>,
    analyses: BTreeMap<(String, String), ValidatedAnalysisSpec>,
}

#[derive(Clone, Debug, PartialEq)]
struct ValidatedCalibration {
    validity_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum ValidatedReplayContract {
    DeterministicBitwise {
        expected_output_sha256_hex: String,
        expected_byte_len: usize,
    },
    NumericalTolerance {
        expected_series: Vec<f64>,
        max_abs_diff: f64,
        max_rel_diff: f64,
        tolerance_justification: String,
    },
    NonReplayableStochastic {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct ValidatedAnalysisSpec {
    method_id: String,
    required_capability_id: String,
    required_capability_version: u32,
    model_id: Option<String>,
    model_version: Option<String>,
    license_incompatible_detail: Option<String>,
    pinned_input_versions: Vec<(String, String)>,
    pinned_asset_paths: Vec<String>,
    calibration_ref: Option<(String, String)>,
    replay_contract: ValidatedReplayContract,
}

impl ValidatedBundle {
    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    pub fn destination_case_id(&self) -> &str {
        &self.destination_case_id
    }

    pub fn local_namespace_prefix(&self) -> &str {
        &self.local_namespace_prefix
    }

    pub fn verified_entries(&self) -> &[VerifiedStagedEntry] {
        &self.verified_entries
    }

    pub fn mapped_records(&self) -> &[MappedBundleRecord] {
        &self.mapped_records
    }

    pub fn omitted_optional_capabilities(&self) -> &[String] {
        &self.omitted_optional_capabilities
    }

    pub fn omission_count(&self) -> usize {
        self.omission_count
    }

    pub fn inert_import_verified(&self) -> bool {
        self.inert_import_verified
    }
}

/// Candidate output supplied to an explicit analysis replay command (`REPRO-03`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CandidateReplayOutput<'input> {
    Bytes(&'input [u8]),
    NumericalSeries(&'input [f64]),
    NotExecuted,
}

/// Explicit request to evaluate or verify replay of a single analysis run from a validated bundle
/// (`REPRO-03`, `A17`). Replay is a separate command and never runs automatically during import.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnalysisReplayRequest<'input> {
    pub analysis_id: &'input str,
    pub version_id: &'input str,
    pub available_models: &'input [(&'input str, &'input str)],
    pub candidate_output: CandidateReplayOutput<'input>,
}

/// Explicit reason why an analysis in a bundle cannot be replayed (`REPRO-03`, `A17`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NonReplayableReason {
    UnsupportedCapability {
        capability_id: String,
        version: u32,
    },
    UnavailableSource {
        record_id: String,
        version_id: String,
        reason: Option<OmissionReason>,
    },
    UnavailableAsset {
        asset_path: String,
    },
    InvalidatedCalibration {
        calibration_id: String,
        version_id: String,
        reason: String,
    },
    MissingOrMismatchedModel {
        model_id: String,
        required_version: String,
        available_version: Option<String>,
    },
    ChangedOrIncompatibleLicense {
        method_id: String,
        detail: String,
    },
    StochasticOutput {
        method_id: String,
        reason: String,
    },
}

/// Outcome of an explicit analysis replay check against a validated bundle (`REPRO-03`, `A17`).
#[derive(Clone, Debug, PartialEq)]
pub enum AnalysisReplayOutcome {
    ReproducedBitwise {
        analysis_id: String,
        version_id: String,
        output_sha256_hex: String,
        byte_len: usize,
    },
    DivergedBitwise {
        analysis_id: String,
        version_id: String,
        expected_sha256_hex: String,
        actual_sha256_hex: String,
        expected_byte_len: usize,
        actual_byte_len: usize,
    },
    ReproducedWithinTolerance {
        analysis_id: String,
        version_id: String,
        sample_count: usize,
        max_observed_abs_diff: f64,
        max_observed_rel_diff: f64,
        max_allowed_abs_diff: f64,
        max_allowed_rel_diff: f64,
        tolerance_justification: String,
    },
    DivergedBeyondTolerance {
        analysis_id: String,
        version_id: String,
        first_failing_index: usize,
        expected_value: f64,
        actual_value: f64,
        abs_diff: f64,
        rel_diff: f64,
        max_allowed_abs_diff: f64,
        max_allowed_rel_diff: f64,
    },
    NonReplayable {
        analysis_id: String,
        version_id: String,
        reason: NonReplayableReason,
    },
}

/// Validates and stages a portable directory bundle in memory according to `REPRO-01`, `REPRO-02`,
/// `A17`, and `docs/contracts/report-export.md` §§5–6.
///
/// This function is strictly side-effect-free: it never writes files, executes code, connects
/// devices, fetches remote URLs, or runs analysis replay.
pub fn validate_and_stage_bundle<'input, F>(
    input: &BundleImportInput<'input>,
    supported_capabilities: &SupportedCapabilities<'input>,
    limits: BundleLimits,
    mut stop_check: F,
) -> Result<ValidatedBundle, BundleError>
where
    F: FnMut() -> Result<(), StopReason>,
{
    check_stop(&mut stop_check)?;

    if input.identity.format != SUPPORTED_BUNDLE_FORMAT
        || input.identity.format_version != SUPPORTED_BUNDLE_FORMAT_VERSION
    {
        return Err(BundleError::UnsupportedBundleFormat {
            format: input.identity.format.to_owned(),
            version: input.identity.format_version,
        });
    }

    let mut text_bytes_used = 0usize;
    validate_identifier(
        input.identity.package_id,
        "identity.package_id",
        &mut text_bytes_used,
        limits,
    )?;
    validate_identifier(
        input.identity.producer_id,
        "identity.producer_id",
        &mut text_bytes_used,
        limits,
    )?;
    validate_text_field(
        input.identity.producer_version,
        "identity.producer_version",
        &mut text_bytes_used,
        limits,
    )?;
    validate_time_display(
        input.identity.created_time,
        "identity.created_time",
        &mut text_bytes_used,
        limits,
    )?;
    validate_identifier(
        input.destination_case_id,
        "destination_case_id",
        &mut text_bytes_used,
        limits,
    )?;
    validate_identifier(
        input.local_namespace_prefix,
        "local_namespace_prefix",
        &mut text_bytes_used,
        limits,
    )?;

    for existing_case_id in input.existing_case_ids {
        check_stop(&mut stop_check)?;
        if *existing_case_id == input.destination_case_id {
            return Err(BundleError::ExistingCaseOverwriteRefused {
                destination_case_id: input.destination_case_id.to_owned(),
            });
        }
    }

    enforce_count_limit(
        BundleLimitKind::Capabilities,
        input.capabilities.len(),
        limits.max_capabilities,
    )?;
    enforce_count_limit(
        BundleLimitKind::Entries,
        input.manifest_entries.len(),
        limits.max_entries,
    )?;
    enforce_count_limit(
        BundleLimitKind::Entries,
        input.staged_files.len(),
        limits.max_entries,
    )?;
    enforce_count_limit(
        BundleLimitKind::Records,
        input.records.len(),
        limits.max_records,
    )?;
    enforce_count_limit(
        BundleLimitKind::References,
        input.references.len(),
        limits.max_references,
    )?;
    enforce_count_limit(
        BundleLimitKind::Omissions,
        input.omissions.len(),
        limits.max_omissions,
    )?;

    let total_repro_items = input
        .queries
        .len()
        .saturating_add(input.groupings.len())
        .saturating_add(input.calibrations.len())
        .saturating_add(input.forecasts.len())
        .saturating_add(input.analyses.len());
    enforce_count_limit(
        BundleLimitKind::ReproductionItems,
        total_repro_items,
        limits.max_reproduction_items,
    )?;

    let mut supported_set = BTreeSet::new();
    for (cap_id, cap_ver) in supported_capabilities.supported {
        supported_set.insert(((*cap_id).to_owned(), *cap_ver));
    }

    let mut declared_capabilities = BTreeMap::new();
    let mut omitted_optional_capabilities = Vec::new();
    for cap in input.capabilities {
        check_stop(&mut stop_check)?;
        validate_identifier(
            cap.capability_id,
            "capability.capability_id",
            &mut text_bytes_used,
            limits,
        )?;
        if cap.version == 0 {
            return Err(BundleError::UnsupportedRequiredCapability {
                capability_id: cap.capability_id.to_owned(),
                version: 0,
            });
        }
        let key = (cap.capability_id.to_owned(), cap.version);
        if declared_capabilities.insert(key.clone(), *cap).is_some() {
            return Err(BundleError::DuplicateIdentifier {
                kind: "capability",
                id: cap.capability_id.to_owned(),
            });
        }
        let is_supported = supported_set.contains(&key);
        if !is_supported {
            match cap.requirement {
                CapabilityRequirement::Required => {
                    return Err(BundleError::UnsupportedRequiredCapability {
                        capability_id: cap.capability_id.to_owned(),
                        version: cap.version,
                    });
                }
                CapabilityRequirement::Optional => {
                    try_push(
                        &mut omitted_optional_capabilities,
                        cap.capability_id.to_owned(),
                    )?;
                }
            }
        }
    }

    let mut manifest_by_path = BTreeMap::new();
    let mut lower_paths = BTreeMap::new();
    for entry in input.manifest_entries {
        check_stop(&mut stop_check)?;
        validate_bundle_relative_path(
            entry.relative_path,
            entry.role,
            &mut text_bytes_used,
            limits,
        )?;
        validate_text_field(
            entry.media_type,
            "manifest_entry.media_type",
            &mut text_bytes_used,
            limits,
        )?;
        validate_sha256_hex(entry.declared_sha256_hex, "manifest_entry.declared_sha256")?;
        charge_text_bytes(
            &mut text_bytes_used,
            entry.declared_sha256_hex.len(),
            limits,
        )?;

        if entry.declared_byte_len > limits.max_file_bytes {
            return Err(BundleError::LimitExceeded {
                limit: BundleLimitKind::FileBytes,
                actual: entry.declared_byte_len,
                max: limits.max_file_bytes,
            });
        }

        if let Some(0) = entry.schema_version {
            return Err(BundleError::InvalidRecordSchema {
                record_id: entry.relative_path.to_owned(),
                reason: "schema_version must be non-zero when present",
            });
        }

        if entry.role == BundleEntryRole::Assets
            && !asset_path_matches_sha256(entry.relative_path, entry.declared_sha256_hex)
        {
            return Err(BundleError::AssetFilenameHashMismatch {
                path: entry.relative_path.to_owned(),
                declared_sha256_hex: entry.declared_sha256_hex.to_owned(),
            });
        }

        let lower = entry.relative_path.to_ascii_lowercase();
        if let Some(existing) = lower_paths.insert(lower, entry.relative_path.to_owned())
            && existing != entry.relative_path
        {
            return Err(BundleError::CaseInsensitivePathCollision {
                first_path: existing,
                colliding_path: entry.relative_path.to_owned(),
            });
        }

        if manifest_by_path
            .insert(entry.relative_path.to_owned(), *entry)
            .is_some()
        {
            return Err(BundleError::DuplicateIdentifier {
                kind: "manifest_entry",
                id: entry.relative_path.to_owned(),
            });
        }
    }

    let mut staged_by_path = BTreeMap::new();
    let mut total_staged_bytes = 0usize;
    for staged in input.staged_files {
        check_stop(&mut stop_check)?;
        if staged.entry_kind != StagedFilesystemEntryKind::RegularFile {
            return Err(BundleError::UnsafeFilesystemEntry {
                path: staged.relative_path.to_owned(),
                entry_kind: staged.entry_kind,
            });
        }
        if let Some(archive_kind) =
            detect_archive_or_compressed_container(staged.relative_path, staged.staged_bytes)
        {
            return Err(BundleError::ArchiveContainerRejected {
                path: staged.relative_path.to_owned(),
                container_kind: archive_kind,
            });
        }
        if !manifest_by_path.contains_key(staged.relative_path) {
            return Err(BundleError::UnlistedStagedPayload {
                path: staged.relative_path.to_owned(),
            });
        }
        if staged.staged_bytes.len() > limits.max_file_bytes {
            return Err(BundleError::LimitExceeded {
                limit: BundleLimitKind::FileBytes,
                actual: staged.staged_bytes.len(),
                max: limits.max_file_bytes,
            });
        }
        total_staged_bytes = total_staged_bytes.saturating_add(staged.staged_bytes.len());
        if total_staged_bytes > limits.max_total_bytes {
            return Err(BundleError::LimitExceeded {
                limit: BundleLimitKind::TotalBytes,
                actual: total_staged_bytes,
                max: limits.max_total_bytes,
            });
        }
        if staged_by_path
            .insert(staged.relative_path.to_owned(), *staged)
            .is_some()
        {
            return Err(BundleError::DuplicateIdentifier {
                kind: "staged_file",
                id: staged.relative_path.to_owned(),
            });
        }
    }

    let mut verified_entries = Vec::new();
    verified_entries
        .try_reserve(input.manifest_entries.len())
        .map_err(|_| BundleError::AllocationFailed)?;
    let mut verified_asset_digests = BTreeMap::new();

    for entry in input.manifest_entries {
        check_stop(&mut stop_check)?;
        let Some(staged) = staged_by_path.get(entry.relative_path) else {
            return Err(BundleError::MissingStagedPayload {
                path: entry.relative_path.to_owned(),
            });
        };
        if staged.staged_bytes.len() != entry.declared_byte_len {
            return Err(BundleError::ByteLengthMismatch {
                path: entry.relative_path.to_owned(),
                declared_len: entry.declared_byte_len,
                actual_len: staged.staged_bytes.len(),
            });
        }
        let actual_sha256 = sha256_hex(staged.staged_bytes);
        if actual_sha256 != entry.declared_sha256_hex {
            return Err(BundleError::IntegrityHashMismatch {
                path: entry.relative_path.to_owned(),
                declared_sha256_hex: entry.declared_sha256_hex.to_owned(),
                actual_sha256_hex: actual_sha256,
            });
        }
        if let Some(rechecked) = staged.rechecked_sha256_hex {
            validate_sha256_hex(rechecked, "staged_file.rechecked_sha256_hex")?;
            if rechecked != actual_sha256 {
                return Err(BundleError::InputChangedDuringStaging {
                    path: entry.relative_path.to_owned(),
                    initial_sha256_hex: actual_sha256,
                    rechecked_sha256_hex: rechecked.to_owned(),
                });
            }
        }
        if entry.role == BundleEntryRole::Assets {
            verified_asset_digests.insert(entry.relative_path.to_owned(), actual_sha256.clone());
        }
        let report_handling = if entry.role == BundleEntryRole::Reports {
            Some(ImportedReportHandling::UntrustedRequiresRegenerationOrInertViewer)
        } else {
            None
        };
        try_push(
            &mut verified_entries,
            VerifiedStagedEntry {
                relative_path: entry.relative_path.to_owned(),
                role: entry.role,
                media_type: entry.media_type.to_owned(),
                verified_byte_len: staged.staged_bytes.len(),
                verified_sha256_hex: actual_sha256,
                report_handling,
            },
        )?;
    }

    let mut use_conditions_by_asset = BTreeMap::new();
    for cond in input.use_conditions {
        check_stop(&mut stop_check)?;
        validate_bundle_relative_path(
            cond.asset_path,
            BundleEntryRole::Assets,
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            cond.source_id,
            "use_conditions.source_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_text_field(
            cond.ordinary_access_method,
            "use_conditions.ordinary_access_method",
            &mut text_bytes_used,
            limits,
        )?;
        validate_text_field(
            cond.attribution_notice,
            "use_conditions.attribution_notice",
            &mut text_bytes_used,
            limits,
        )?;
        if !verified_asset_digests.contains_key(cond.asset_path) {
            return Err(BundleError::UnexplainedDanglingReference {
                from_id: cond.source_id.to_owned(),
                target_id: cond.asset_path.to_owned(),
            });
        }
        if use_conditions_by_asset
            .insert(cond.asset_path.to_owned(), *cond)
            .is_some()
        {
            return Err(BundleError::DuplicateIdentifier {
                kind: "asset_use_conditions",
                id: cond.asset_path.to_owned(),
            });
        }
    }

    let mut omitted_record_versions = BTreeMap::new();
    let mut seen_omission_ids = BTreeSet::new();
    for omission in input.omissions {
        check_stop(&mut stop_check)?;
        validate_identifier(
            omission.omitted_id,
            "omission.omitted_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            omission.source_id,
            "omission.source_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            omission.version_id,
            "omission.version_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_text_field(
            omission.coverage_effect,
            "omission.coverage_effect",
            &mut text_bytes_used,
            limits,
        )?;
        if let Some(note) = omission.unreproducible_step_note {
            validate_text_field(
                note,
                "omission.unreproducible_step_note",
                &mut text_bytes_used,
                limits,
            )?;
        }
        if !seen_omission_ids.insert(omission.omitted_id.to_owned()) {
            return Err(BundleError::DuplicateIdentifier {
                kind: "omission",
                id: omission.omitted_id.to_owned(),
            });
        }
        omitted_record_versions.insert(
            (
                omission.omitted_id.to_owned(),
                omission.version_id.to_owned(),
            ),
            omission.reason,
        );
    }

    let mut mapped_records = Vec::new();
    mapped_records
        .try_reserve(input.records.len())
        .map_err(|_| BundleError::AllocationFailed)?;
    let mut available_record_versions = BTreeSet::new();
    let mut seen_record_versions = BTreeSet::new();

    for rec in input.records {
        check_stop(&mut stop_check)?;
        validate_identifier(
            rec.origin_namespace,
            "record.origin_namespace",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            rec.record_id,
            "record.record_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            rec.version_id,
            "record.version_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            rec.origin_actor_id,
            "record.origin_actor_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_time_display(
            rec.recorded_time,
            "record.recorded_time",
            &mut text_bytes_used,
            limits,
        )?;
        if let Some(uncertainty) = rec.uncertainty_note {
            validate_text_field(
                uncertainty,
                "record.uncertainty_note",
                &mut text_bytes_used,
                limits,
            )?;
        }

        let rec_key = (rec.record_id.to_owned(), rec.version_id.to_owned());
        if !seen_record_versions.insert(rec_key.clone()) {
            return Err(BundleError::DuplicateIdentifier {
                kind: "record_version",
                id: format!("{}@{}", rec.record_id, rec.version_id),
            });
        }

        let asset_sha256_hex = if let Some(asset_path) = rec.asset_relative_path {
            let Some(digest) = verified_asset_digests.get(asset_path) else {
                return Err(BundleError::UnexplainedDanglingReference {
                    from_id: rec.record_id.to_owned(),
                    target_id: asset_path.to_owned(),
                });
            };
            Some(digest.clone())
        } else {
            None
        };

        let optional_handling = if let Some(cap_id) = rec.required_capability_id {
            validate_identifier(
                cap_id,
                "record.required_capability_id",
                &mut text_bytes_used,
                limits,
            )?;
            let is_supported = supported_set.iter().any(|(id, _)| id == cap_id);
            if is_supported {
                OptionalRecordHandling::Supported
            } else if supported_capabilities.allow_inert_optional_records {
                OptionalRecordHandling::RetainedInertOpaqueBytes {
                    capability_id: cap_id.to_owned(),
                }
            } else {
                OptionalRecordHandling::OmittedWithNotice {
                    capability_id: cap_id.to_owned(),
                }
            }
        } else {
            OptionalRecordHandling::Supported
        };

        if matches!(optional_handling, OptionalRecordHandling::Supported) {
            available_record_versions.insert(rec_key);
        } else {
            omitted_record_versions.insert(rec_key, OmissionReason::Unsupported);
        }

        let local_record_id = format!("{}/rec/{}", input.local_namespace_prefix, rec.record_id);
        let local_version_id = format!("{}/ver/{}", input.local_namespace_prefix, rec.version_id);

        try_push(
            &mut mapped_records,
            MappedBundleRecord {
                origin_namespace: rec.origin_namespace.to_owned(),
                origin_record_id: rec.record_id.to_owned(),
                origin_version_id: rec.version_id.to_owned(),
                local_record_id,
                local_version_id,
                kind: rec.kind,
                origin_class: rec.origin_class,
                review_status: rec.review_status,
                attributed_actor: AttributedOriginActor {
                    origin_namespace: rec.origin_namespace.to_owned(),
                    origin_actor_id: rec.origin_actor_id.to_owned(),
                    local_authority: LocalAuthorityGrant::None,
                },
                asset_sha256_hex,
                optional_handling,
            },
        )?;
    }

    let mut seen_ref_ids = BTreeSet::new();
    for reference in input.references {
        check_stop(&mut stop_check)?;
        validate_identifier(
            reference.reference_id,
            "reference.reference_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            reference.from_record_id,
            "reference.from_record_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            reference.from_version_id,
            "reference.from_version_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            reference.relation,
            "reference.relation",
            &mut text_bytes_used,
            limits,
        )?;
        if !seen_ref_ids.insert(reference.reference_id.to_owned()) {
            return Err(BundleError::DuplicateIdentifier {
                kind: "reference",
                id: reference.reference_id.to_owned(),
            });
        }
        let from_key = (
            reference.from_record_id.to_owned(),
            reference.from_version_id.to_owned(),
        );
        if !seen_record_versions.contains(&from_key) {
            return Err(BundleError::UnexplainedDanglingReference {
                from_id: reference.reference_id.to_owned(),
                target_id: format!("{}@{}", reference.from_record_id, reference.from_version_id),
            });
        }
        match reference.target {
            BundleReferenceTarget::IncludedRecord {
                target_record_id,
                target_version_id,
            } => {
                validate_identifier(
                    target_record_id,
                    "reference.target_record_id",
                    &mut text_bytes_used,
                    limits,
                )?;
                validate_identifier(
                    target_version_id,
                    "reference.target_version_id",
                    &mut text_bytes_used,
                    limits,
                )?;
                let target_key = (target_record_id.to_owned(), target_version_id.to_owned());
                if !seen_record_versions.contains(&target_key) {
                    return Err(BundleError::UnexplainedDanglingReference {
                        from_id: reference.reference_id.to_owned(),
                        target_id: format!("{target_record_id}@{target_version_id}"),
                    });
                }
            }
            BundleReferenceTarget::ExternalReferenceOnly {
                target_record_id,
                target_version_id,
                source_id,
                external_url,
                ..
            } => {
                validate_identifier(
                    target_record_id,
                    "reference.target_record_id",
                    &mut text_bytes_used,
                    limits,
                )?;
                validate_identifier(
                    target_version_id,
                    "reference.target_version_id",
                    &mut text_bytes_used,
                    limits,
                )?;
                validate_identifier(
                    source_id,
                    "reference.source_id",
                    &mut text_bytes_used,
                    limits,
                )?;
                if let Some(url) = external_url {
                    validate_safe_external_url(
                        url,
                        reference.reference_id,
                        &mut text_bytes_used,
                        limits,
                    )?;
                }
            }
            BundleReferenceTarget::PermittedUnavailable {
                target_record_id,
                target_version_id,
                coverage_note,
                ..
            } => {
                validate_identifier(
                    target_record_id,
                    "reference.target_record_id",
                    &mut text_bytes_used,
                    limits,
                )?;
                validate_identifier(
                    target_version_id,
                    "reference.target_version_id",
                    &mut text_bytes_used,
                    limits,
                )?;
                validate_text_field(
                    coverage_note,
                    "reference.coverage_note",
                    &mut text_bytes_used,
                    limits,
                )?;
            }
        }
    }

    for query in input.queries {
        check_stop(&mut stop_check)?;
        validate_identifier(
            query.query_id,
            "query.query_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            query.version_id,
            "query.version_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_no_side_effect_hooks(
            query.query_expression,
            query.query_id,
            &mut text_bytes_used,
            limits,
        )?;
        validate_text_field(
            query.coverage_note,
            "query.coverage_note",
            &mut text_bytes_used,
            limits,
        )?;
        validate_pinned_version_refs(
            query.query_id,
            query.pinned_input_versions,
            &seen_record_versions,
            &omitted_record_versions,
            &mut text_bytes_used,
            limits,
        )?;
    }

    for grouping in input.groupings {
        check_stop(&mut stop_check)?;
        validate_identifier(
            grouping.grouping_id,
            "grouping.grouping_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            grouping.version_id,
            "grouping.version_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_text_field(
            grouping.rationale,
            "grouping.rationale",
            &mut text_bytes_used,
            limits,
        )?;
        validate_pinned_version_refs(
            grouping.grouping_id,
            grouping.member_record_versions,
            &seen_record_versions,
            &omitted_record_versions,
            &mut text_bytes_used,
            limits,
        )?;
    }

    let mut calibrations = BTreeMap::new();
    for cal in input.calibrations {
        check_stop(&mut stop_check)?;
        validate_identifier(
            cal.calibration_id,
            "calibration.calibration_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            cal.version_id,
            "calibration.version_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            cal.sensor_or_view_id,
            "calibration.sensor_or_view_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_text_field(
            cal.reference_description,
            "calibration.reference_description",
            &mut text_bytes_used,
            limits,
        )?;
        validate_text_field(
            cal.residual_unit,
            "calibration.residual_unit",
            &mut text_bytes_used,
            limits,
        )?;
        if !cal.residual_rms.is_finite() || cal.residual_rms < 0.0 {
            return Err(BundleError::InvalidRecordSchema {
                record_id: cal.calibration_id.to_owned(),
                reason: "calibration residual_rms must be finite and non-negative",
            });
        }
        let validity_error = match cal.validity {
            CalibrationValidity::Valid => None,
            CalibrationValidity::Invalidated { reason } => {
                validate_text_field(
                    reason,
                    "calibration.invalidated_reason",
                    &mut text_bytes_used,
                    limits,
                )?;
                Some(reason.to_owned())
            }
        };
        let key = (cal.calibration_id.to_owned(), cal.version_id.to_owned());
        if calibrations
            .insert(key, ValidatedCalibration { validity_error })
            .is_some()
        {
            return Err(BundleError::DuplicateIdentifier {
                kind: "calibration",
                id: format!("{}@{}", cal.calibration_id, cal.version_id),
            });
        }
    }

    for forecast in input.forecasts {
        check_stop(&mut stop_check)?;
        validate_identifier(
            forecast.forecast_id,
            "forecast.forecast_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            forecast.version_id,
            "forecast.version_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_time_display(
            forecast.cutoff_time,
            "forecast.cutoff_time",
            &mut text_bytes_used,
            limits,
        )?;
        validate_text_field(
            forecast.prediction_summary,
            "forecast.prediction_summary",
            &mut text_bytes_used,
            limits,
        )?;
        validate_pinned_version_refs(
            forecast.forecast_id,
            forecast.pinned_input_versions,
            &seen_record_versions,
            &omitted_record_versions,
            &mut text_bytes_used,
            limits,
        )?;
    }

    let mut analyses = BTreeMap::new();
    for analysis in input.analyses {
        check_stop(&mut stop_check)?;
        validate_identifier(
            analysis.analysis_id,
            "analysis.analysis_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            analysis.version_id,
            "analysis.version_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            analysis.method_id,
            "analysis.method_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_identifier(
            analysis.required_capability_id,
            "analysis.required_capability_id",
            &mut text_bytes_used,
            limits,
        )?;
        validate_no_side_effect_hooks(
            analysis.configuration_summary,
            analysis.analysis_id,
            &mut text_bytes_used,
            limits,
        )?;
        if let Some(model_id) = analysis.model_id {
            validate_identifier(model_id, "analysis.model_id", &mut text_bytes_used, limits)?;
        }
        if let Some(model_ver) = analysis.model_version {
            validate_identifier(
                model_ver,
                "analysis.model_version",
                &mut text_bytes_used,
                limits,
            )?;
        }
        let license_incompatible_detail = match analysis.license_status {
            AnalysisLicenseStatus::Compatible { license_id } => {
                validate_text_field(
                    license_id,
                    "analysis.license_id",
                    &mut text_bytes_used,
                    limits,
                )?;
                None
            }
            AnalysisLicenseStatus::ChangedOrIncompatible { detail } => {
                validate_text_field(
                    detail,
                    "analysis.license_detail",
                    &mut text_bytes_used,
                    limits,
                )?;
                Some(detail.to_owned())
            }
        };

        validate_pinned_version_refs(
            analysis.analysis_id,
            analysis.pinned_input_versions,
            &seen_record_versions,
            &omitted_record_versions,
            &mut text_bytes_used,
            limits,
        )?;

        let mut pinned_asset_paths = Vec::new();
        for asset_path in analysis.pinned_asset_paths {
            validate_bundle_relative_path(
                asset_path,
                BundleEntryRole::Assets,
                &mut text_bytes_used,
                limits,
            )?;
            try_push(&mut pinned_asset_paths, (*asset_path).to_owned())?;
        }

        let calibration_ref = if let Some((cal_id, cal_ver)) = analysis.calibration_ref {
            validate_identifier(
                cal_id,
                "analysis.calibration_id",
                &mut text_bytes_used,
                limits,
            )?;
            validate_identifier(
                cal_ver,
                "analysis.calibration_version",
                &mut text_bytes_used,
                limits,
            )?;
            let cal_key = (cal_id.to_owned(), cal_ver.to_owned());
            if !calibrations.contains_key(&cal_key) {
                return Err(BundleError::UnexplainedDanglingReference {
                    from_id: analysis.analysis_id.to_owned(),
                    target_id: format!("{cal_id}@{cal_ver}"),
                });
            }
            Some(cal_key)
        } else {
            None
        };

        let replay_contract = match analysis.replay_contract {
            ReplayContract::DeterministicBitwise {
                expected_output_sha256_hex,
                expected_byte_len,
            } => {
                validate_sha256_hex(
                    expected_output_sha256_hex,
                    "analysis.expected_output_sha256_hex",
                )?;
                ValidatedReplayContract::DeterministicBitwise {
                    expected_output_sha256_hex: expected_output_sha256_hex.to_owned(),
                    expected_byte_len,
                }
            }
            ReplayContract::NumericalTolerance {
                expected_series,
                max_abs_diff,
                max_rel_diff,
                tolerance_justification,
            } => {
                validate_text_field(
                    tolerance_justification,
                    "analysis.tolerance_justification",
                    &mut text_bytes_used,
                    limits,
                )?;
                if expected_series.is_empty() {
                    return Err(BundleError::InvalidToleranceConfiguration {
                        analysis_id: analysis.analysis_id.to_owned(),
                        reason: "expected_series must not be empty",
                    });
                }
                enforce_count_limit(
                    BundleLimitKind::ReplaySamples,
                    expected_series.len(),
                    limits.max_replay_samples,
                )?;
                if !max_abs_diff.is_finite()
                    || !max_rel_diff.is_finite()
                    || max_abs_diff < 0.0
                    || max_rel_diff < 0.0
                {
                    return Err(BundleError::InvalidToleranceConfiguration {
                        analysis_id: analysis.analysis_id.to_owned(),
                        reason: "max_abs_diff and max_rel_diff must be finite and non-negative",
                    });
                }
                for val in expected_series {
                    if !val.is_finite() {
                        return Err(BundleError::InvalidToleranceConfiguration {
                            analysis_id: analysis.analysis_id.to_owned(),
                            reason: "expected_series values must be finite",
                        });
                    }
                }
                ValidatedReplayContract::NumericalTolerance {
                    expected_series: expected_series.to_vec(),
                    max_abs_diff,
                    max_rel_diff,
                    tolerance_justification: tolerance_justification.to_owned(),
                }
            }
            ReplayContract::NonReplayableStochastic { reason } => {
                validate_text_field(
                    reason,
                    "analysis.stochastic_reason",
                    &mut text_bytes_used,
                    limits,
                )?;
                ValidatedReplayContract::NonReplayableStochastic {
                    reason: reason.to_owned(),
                }
            }
        };

        let mut pinned_inputs = Vec::new();
        for (rec_id, ver_id) in analysis.pinned_input_versions {
            try_push(
                &mut pinned_inputs,
                ((*rec_id).to_owned(), (*ver_id).to_owned()),
            )?;
        }

        let key = (
            analysis.analysis_id.to_owned(),
            analysis.version_id.to_owned(),
        );
        if analyses
            .insert(
                key,
                ValidatedAnalysisSpec {
                    method_id: analysis.method_id.to_owned(),
                    required_capability_id: analysis.required_capability_id.to_owned(),
                    required_capability_version: analysis.required_capability_version,
                    model_id: analysis.model_id.map(str::to_owned),
                    model_version: analysis.model_version.map(str::to_owned),
                    license_incompatible_detail,
                    pinned_input_versions: pinned_inputs,
                    pinned_asset_paths,
                    calibration_ref,
                    replay_contract,
                },
            )
            .is_some()
        {
            return Err(BundleError::DuplicateIdentifier {
                kind: "analysis",
                id: format!("{}@{}", analysis.analysis_id, analysis.version_id),
            });
        }
    }

    check_stop(&mut stop_check)?;

    Ok(ValidatedBundle {
        package_id: input.identity.package_id.to_owned(),
        destination_case_id: input.destination_case_id.to_owned(),
        local_namespace_prefix: input.local_namespace_prefix.to_owned(),
        verified_entries,
        mapped_records,
        omitted_optional_capabilities,
        omission_count: input.omissions.len(),
        inert_import_verified: true,
        calibrations,
        available_record_versions,
        omitted_record_versions,
        verified_asset_digests,
        supported_capabilities: supported_set,
        analyses,
    })
}

/// Evaluates an explicit analysis replay request against a `ValidatedBundle` (`REPRO-03`, `A17`).
///
/// Missing models, unavailable/omitted sources, invalidated calibrations, changed licenses, and
/// stochastic methods are reported as explicit `AnalysisReplayOutcome::NonReplayable` variants
/// rather than silently substituted.
pub fn replay_analysis<F>(
    bundle: &ValidatedBundle,
    request: &AnalysisReplayRequest<'_>,
    limits: BundleLimits,
    mut stop_check: F,
) -> Result<AnalysisReplayOutcome, BundleError>
where
    F: FnMut() -> Result<(), StopReason>,
{
    check_stop(&mut stop_check)?;

    let key = (
        request.analysis_id.to_owned(),
        request.version_id.to_owned(),
    );
    let Some(spec) = bundle.analyses.get(&key) else {
        return Err(BundleError::UnknownAnalysisRun {
            analysis_id: request.analysis_id.to_owned(),
            version_id: request.version_id.to_owned(),
        });
    };

    let cap_key = (
        spec.required_capability_id.clone(),
        spec.required_capability_version,
    );
    if !bundle.supported_capabilities.contains(&cap_key) {
        return Ok(AnalysisReplayOutcome::NonReplayable {
            analysis_id: request.analysis_id.to_owned(),
            version_id: request.version_id.to_owned(),
            reason: NonReplayableReason::UnsupportedCapability {
                capability_id: spec.required_capability_id.clone(),
                version: spec.required_capability_version,
            },
        });
    }

    for (rec_id, ver_id) in &spec.pinned_input_versions {
        check_stop(&mut stop_check)?;
        let input_key = (rec_id.clone(), ver_id.clone());
        if !bundle.available_record_versions.contains(&input_key) {
            let reason = bundle.omitted_record_versions.get(&input_key).copied();
            return Ok(AnalysisReplayOutcome::NonReplayable {
                analysis_id: request.analysis_id.to_owned(),
                version_id: request.version_id.to_owned(),
                reason: NonReplayableReason::UnavailableSource {
                    record_id: rec_id.clone(),
                    version_id: ver_id.clone(),
                    reason,
                },
            });
        }
    }

    for asset_path in &spec.pinned_asset_paths {
        check_stop(&mut stop_check)?;
        if !bundle.verified_asset_digests.contains_key(asset_path) {
            return Ok(AnalysisReplayOutcome::NonReplayable {
                analysis_id: request.analysis_id.to_owned(),
                version_id: request.version_id.to_owned(),
                reason: NonReplayableReason::UnavailableAsset {
                    asset_path: asset_path.clone(),
                },
            });
        }
    }

    if let Some(cal_key) = &spec.calibration_ref
        && let Some(cal) = bundle.calibrations.get(cal_key)
        && let Some(reason) = &cal.validity_error
    {
        return Ok(AnalysisReplayOutcome::NonReplayable {
            analysis_id: request.analysis_id.to_owned(),
            version_id: request.version_id.to_owned(),
            reason: NonReplayableReason::InvalidatedCalibration {
                calibration_id: cal_key.0.clone(),
                version_id: cal_key.1.clone(),
                reason: reason.clone(),
            },
        });
    }

    if let Some(detail) = &spec.license_incompatible_detail {
        return Ok(AnalysisReplayOutcome::NonReplayable {
            analysis_id: request.analysis_id.to_owned(),
            version_id: request.version_id.to_owned(),
            reason: NonReplayableReason::ChangedOrIncompatibleLicense {
                method_id: spec.method_id.clone(),
                detail: detail.clone(),
            },
        });
    }

    if let (Some(req_model_id), Some(req_model_ver)) = (&spec.model_id, &spec.model_version) {
        let mut matched_version = None;
        let mut exact_match = false;
        for (avail_id, avail_ver) in request.available_models {
            if *avail_id == req_model_id {
                matched_version = Some((*avail_ver).to_owned());
                if *avail_ver == req_model_ver {
                    exact_match = true;
                    break;
                }
            }
        }
        if !exact_match {
            return Ok(AnalysisReplayOutcome::NonReplayable {
                analysis_id: request.analysis_id.to_owned(),
                version_id: request.version_id.to_owned(),
                reason: NonReplayableReason::MissingOrMismatchedModel {
                    model_id: req_model_id.clone(),
                    required_version: req_model_ver.clone(),
                    available_version: matched_version,
                },
            });
        }
    }

    match &spec.replay_contract {
        ValidatedReplayContract::NonReplayableStochastic { reason } => {
            Ok(AnalysisReplayOutcome::NonReplayable {
                analysis_id: request.analysis_id.to_owned(),
                version_id: request.version_id.to_owned(),
                reason: NonReplayableReason::StochasticOutput {
                    method_id: spec.method_id.clone(),
                    reason: reason.clone(),
                },
            })
        }
        ValidatedReplayContract::DeterministicBitwise {
            expected_output_sha256_hex,
            expected_byte_len,
        } => {
            let CandidateReplayOutput::Bytes(candidate_bytes) = request.candidate_output else {
                return Err(BundleError::InvalidReplayInput {
                    analysis_id: request.analysis_id.to_owned(),
                    reason: "deterministic bitwise replay requires CandidateReplayOutput::Bytes",
                });
            };
            if candidate_bytes.len() > limits.max_file_bytes {
                return Err(BundleError::LimitExceeded {
                    limit: BundleLimitKind::FileBytes,
                    actual: candidate_bytes.len(),
                    max: limits.max_file_bytes,
                });
            }
            let actual_sha256_hex = sha256_hex(candidate_bytes);
            if candidate_bytes.len() == *expected_byte_len
                && &actual_sha256_hex == expected_output_sha256_hex
            {
                Ok(AnalysisReplayOutcome::ReproducedBitwise {
                    analysis_id: request.analysis_id.to_owned(),
                    version_id: request.version_id.to_owned(),
                    output_sha256_hex: actual_sha256_hex,
                    byte_len: candidate_bytes.len(),
                })
            } else {
                Ok(AnalysisReplayOutcome::DivergedBitwise {
                    analysis_id: request.analysis_id.to_owned(),
                    version_id: request.version_id.to_owned(),
                    expected_sha256_hex: expected_output_sha256_hex.clone(),
                    actual_sha256_hex,
                    expected_byte_len: *expected_byte_len,
                    actual_byte_len: candidate_bytes.len(),
                })
            }
        }
        ValidatedReplayContract::NumericalTolerance {
            expected_series,
            max_abs_diff,
            max_rel_diff,
            tolerance_justification,
        } => {
            let CandidateReplayOutput::NumericalSeries(candidate_series) = request.candidate_output
            else {
                return Err(BundleError::InvalidReplayInput {
                    analysis_id: request.analysis_id.to_owned(),
                    reason: "numerical tolerance replay requires CandidateReplayOutput::NumericalSeries",
                });
            };
            enforce_count_limit(
                BundleLimitKind::ReplaySamples,
                candidate_series.len(),
                limits.max_replay_samples,
            )?;
            if candidate_series.len() != expected_series.len() {
                return Err(BundleError::InvalidReplayInput {
                    analysis_id: request.analysis_id.to_owned(),
                    reason: "candidate_series length does not match expected_series length",
                });
            }

            let mut max_observed_abs_diff = 0.0f64;
            let mut max_observed_rel_diff = 0.0f64;

            for (idx, (expected, actual)) in expected_series
                .iter()
                .zip(candidate_series.iter())
                .enumerate()
            {
                check_stop(&mut stop_check)?;
                if !actual.is_finite() {
                    return Err(BundleError::InvalidReplayInput {
                        analysis_id: request.analysis_id.to_owned(),
                        reason: "candidate_series contains non-finite value",
                    });
                }
                let abs_diff = (*actual - *expected).abs();
                let denom = expected.abs().max(actual.abs());
                let rel_diff = if denom == 0.0 { 0.0 } else { abs_diff / denom };

                if abs_diff > *max_abs_diff && rel_diff > *max_rel_diff {
                    return Ok(AnalysisReplayOutcome::DivergedBeyondTolerance {
                        analysis_id: request.analysis_id.to_owned(),
                        version_id: request.version_id.to_owned(),
                        first_failing_index: idx,
                        expected_value: *expected,
                        actual_value: *actual,
                        abs_diff,
                        rel_diff,
                        max_allowed_abs_diff: *max_abs_diff,
                        max_allowed_rel_diff: *max_rel_diff,
                    });
                }
                if abs_diff > max_observed_abs_diff {
                    max_observed_abs_diff = abs_diff;
                }
                if rel_diff > max_observed_rel_diff {
                    max_observed_rel_diff = rel_diff;
                }
            }

            Ok(AnalysisReplayOutcome::ReproducedWithinTolerance {
                analysis_id: request.analysis_id.to_owned(),
                version_id: request.version_id.to_owned(),
                sample_count: expected_series.len(),
                max_observed_abs_diff,
                max_observed_rel_diff,
                max_allowed_abs_diff: *max_abs_diff,
                max_allowed_rel_diff: *max_rel_diff,
                tolerance_justification: tolerance_justification.clone(),
            })
        }
    }
}

/// Computes the FIPS 180-4 SHA-256 digest of `bytes` as a 32-byte array without external crates
/// or unchecked indexing.
pub fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    let (blocks, rem) = bytes.as_chunks::<64>();
    for block in blocks {
        compress_sha256_block(&mut state, block, &K);
    }

    let mut tail = [0u8; 128];
    for (dst, src) in tail.iter_mut().zip(rem.iter()) {
        *dst = *src;
    }
    if let Some(pad_byte) = tail.get_mut(rem.len()) {
        *pad_byte = 0x80;
    }

    let len_bytes = bit_len.to_be_bytes();
    if rem.len() < 56 {
        if let Some(len_slice) = tail.get_mut(56..64) {
            len_slice.copy_from_slice(&len_bytes);
        }
        if let Some(first_block) = tail.get(0..64) {
            compress_sha256_block(&mut state, first_block, &K);
        }
    } else {
        if let Some(len_slice) = tail.get_mut(120..128) {
            len_slice.copy_from_slice(&len_bytes);
        }
        if let Some(first_block) = tail.get(0..64) {
            compress_sha256_block(&mut state, first_block, &K);
        }
        if let Some(second_block) = tail.get(64..128) {
            compress_sha256_block(&mut state, second_block, &K);
        }
    }

    let mut out = [0u8; 32];
    let (dst_chunks, _) = out.as_chunks_mut::<4>();
    for (word, dst_chunk) in state.iter().zip(dst_chunks.iter_mut()) {
        *dst_chunk = word.to_be_bytes();
    }
    out
}

fn compress_sha256_block(state: &mut [u32; 8], block: &[u8], k_table: &[u32; 64]) {
    let mut w = [0u32; 64];
    let (words4, _) = block.as_chunks::<4>();
    for (word_slot, chunk) in w.iter_mut().take(16).zip(words4.iter()) {
        *word_slot = u32::from_be_bytes(*chunk);
    }
    for idx in 16..64 {
        let w_15 = w.get(idx - 15).copied().unwrap_or(0);
        let w_2 = w.get(idx - 2).copied().unwrap_or(0);
        let w_16 = w.get(idx - 16).copied().unwrap_or(0);
        let w_7 = w.get(idx - 7).copied().unwrap_or(0);
        let s0 = w_15.rotate_right(7) ^ w_15.rotate_right(18) ^ (w_15 >> 3);
        let s1 = w_2.rotate_right(17) ^ w_2.rotate_right(19) ^ (w_2 >> 10);
        if let Some(slot) = w.get_mut(idx) {
            *slot = w_16.wrapping_add(s0).wrapping_add(w_7).wrapping_add(s1);
        }
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;

    for (&w_val, &k_val) in w.iter().zip(k_table.iter()) {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(k_val)
            .wrapping_add(w_val);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }

    let [s0, s1, s2, s3, s4, s5, s6, s7] = *state;
    *state = [
        s0.wrapping_add(a),
        s1.wrapping_add(b),
        s2.wrapping_add(c),
        s3.wrapping_add(d),
        s4.wrapping_add(e),
        s5.wrapping_add(f),
        s6.wrapping_add(g),
        s7.wrapping_add(h),
    ];
}

/// Computes the 64-character lowercase hexadecimal SHA-256 digest of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: [char; 16] = [
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
    ];
    let digest = sha256_bytes(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        let hi = usize::from(byte >> 4);
        let lo = usize::from(byte & 0x0f);
        out.push(HEX.get(hi).copied().unwrap_or('0'));
        out.push(HEX.get(lo).copied().unwrap_or('0'));
    }
    out
}

fn check_stop<F>(stop_check: &mut F) -> Result<(), BundleError>
where
    F: FnMut() -> Result<(), StopReason>,
{
    stop_check().map_err(BundleError::Stopped)
}

fn enforce_count_limit(
    limit: BundleLimitKind,
    actual: usize,
    max: usize,
) -> Result<(), BundleError> {
    if actual > max {
        Err(BundleError::LimitExceeded { limit, actual, max })
    } else {
        Ok(())
    }
}

fn charge_text_bytes(
    used: &mut usize,
    additional: usize,
    limits: BundleLimits,
) -> Result<(), BundleError> {
    *used = used.saturating_add(additional);
    if *used > limits.max_text_bytes {
        Err(BundleError::LimitExceeded {
            limit: BundleLimitKind::TextBytes,
            actual: *used,
            max: limits.max_text_bytes,
        })
    } else {
        Ok(())
    }
}

fn try_push<T>(vec: &mut Vec<T>, item: T) -> Result<(), BundleError> {
    vec.try_reserve(1)
        .map_err(|_| BundleError::AllocationFailed)?;
    vec.push(item);
    Ok(())
}

fn contains_bidi_or_forbidden_invisible(text: &str) -> bool {
    text.chars().any(|ch| {
        ch.is_control()
            || matches!(
                ch,
                '\u{0085}'
                    | '\u{061C}'
                    | '\u{180E}'
                    | '\u{200B}'
                    | '\u{200E}'
                    | '\u{200F}'
                    | '\u{2028}'..='\u{202E}'
                    | '\u{2060}'..='\u{2069}'
                    | '\u{3164}'
                    | '\u{FEFF}'
                    | '\u{FFF9}'..='\u{FFFB}'
                    | '\u{E0000}'..='\u{E007F}'
            )
    })
}

fn validate_identifier(
    value: &str,
    field: &'static str,
    text_bytes_used: &mut usize,
    limits: BundleLimits,
) -> Result<(), BundleError> {
    if value.trim().is_empty() {
        return Err(BundleError::EmptyField { field });
    }
    charge_text_bytes(text_bytes_used, value.len(), limits)?;
    if contains_bidi_or_forbidden_invisible(value) {
        return Err(BundleError::InvalidTextEncoding {
            field,
            reason: "contains control, line-break, bidi, or invisible formatting character",
        });
    }
    if looks_like_local_filesystem_path(value) {
        return Err(BundleError::ForbiddenBundleSideEffect {
            context: field.to_owned(),
            reason: "contains local filesystem path",
        });
    }
    Ok(())
}

fn validate_text_field(
    value: &str,
    field: &'static str,
    text_bytes_used: &mut usize,
    limits: BundleLimits,
) -> Result<(), BundleError> {
    if value.trim().is_empty() {
        return Err(BundleError::EmptyField { field });
    }
    charge_text_bytes(text_bytes_used, value.len(), limits)?;
    if contains_bidi_or_forbidden_invisible(value) {
        return Err(BundleError::InvalidTextEncoding {
            field,
            reason: "contains control, line-break, bidi, or invisible formatting character",
        });
    }
    if looks_like_local_filesystem_path(value) {
        return Err(BundleError::ForbiddenBundleSideEffect {
            context: field.to_owned(),
            reason: "contains local filesystem path",
        });
    }
    Ok(())
}

fn validate_time_display(
    time: TimeDisplay<'_>,
    field: &'static str,
    text_bytes_used: &mut usize,
    limits: BundleLimits,
) -> Result<(), BundleError> {
    validate_text_field(time.text, field, text_bytes_used, limits)?;
    validate_text_field(time.time_reference, field, text_bytes_used, limits)?;
    match time.precision {
        TimePrecision::DateOnly => {
            if time.text.contains('T') || time.text.contains(':') {
                return Err(BundleError::InvalidTimeDisplay {
                    field,
                    reason: "date-only timestamp must not include time-of-day components",
                });
            }
        }
        TimePrecision::Unknown { reason } => {
            validate_text_field(reason, field, text_bytes_used, limits)?;
        }
        TimePrecision::ExactInstant
        | TimePrecision::Minute
        | TimePrecision::Hour
        | TimePrecision::ApproximateInterval => {}
    }
    Ok(())
}

fn validate_sha256_hex(value: &str, context: &'static str) -> Result<(), BundleError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(BundleError::InvalidSha256Hex {
            context,
            value: value.to_owned(),
        });
    }
    Ok(())
}

fn validate_bundle_relative_path(
    path: &str,
    role: BundleEntryRole,
    text_bytes_used: &mut usize,
    limits: BundleLimits,
) -> Result<(), BundleError> {
    if path.is_empty() {
        return Err(BundleError::InvalidRelativePath {
            path: String::new(),
            reason: "path must not be empty",
        });
    }
    charge_text_bytes(text_bytes_used, path.len(), limits)?;
    if path.starts_with('/') || path.ends_with('/') {
        return Err(BundleError::InvalidRelativePath {
            path: path.to_owned(),
            reason: "path must be relative and must not start or end with `/`",
        });
    }
    if path.contains('\\') || path.contains(':') {
        return Err(BundleError::InvalidRelativePath {
            path: path.to_owned(),
            reason: "path must not contain backslashes, drive letters, or colons",
        });
    }
    if contains_bidi_or_forbidden_invisible(path) {
        return Err(BundleError::InvalidRelativePath {
            path: path.to_owned(),
            reason: "path contains control, bidi, or invisible characters",
        });
    }
    if !path.starts_with(role.required_prefix()) {
        return Err(BundleError::InvalidRelativePath {
            path: path.to_owned(),
            reason: "path does not reside under the directory prefix required for its role",
        });
    }

    let mut component_count = 0usize;
    for comp in path.split('/') {
        component_count = component_count.saturating_add(1);
        if comp.is_empty() || comp == "." || comp == ".." {
            return Err(BundleError::InvalidRelativePath {
                path: path.to_owned(),
                reason: "path contains empty, dot, or parent (`..`) traversal component",
            });
        }
        if comp.ends_with('.') || comp.ends_with(' ') || comp.starts_with(' ') {
            return Err(BundleError::InvalidRelativePath {
                path: path.to_owned(),
                reason: "path component has leading/trailing space or trailing dot",
            });
        }
        if is_windows_reserved_device_name(comp) {
            return Err(BundleError::InvalidRelativePath {
                path: path.to_owned(),
                reason: "path component uses a reserved Windows device name",
            });
        }
    }
    if component_count < 2 {
        return Err(BundleError::InvalidRelativePath {
            path: path.to_owned(),
            reason: "path must include a layout directory and filename",
        });
    }
    Ok(())
}

fn is_windows_reserved_device_name(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn asset_path_matches_sha256(path: &str, declared_sha256_hex: &str) -> bool {
    let Some(filename) = path.rsplit('/').next() else {
        return false;
    };
    let stem = filename.split('.').next().unwrap_or(filename);
    stem == declared_sha256_hex
}

fn detect_archive_or_compressed_container(path: &str, bytes: &[u8]) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        return Some("ZIP archive extension");
    }
    if lower.ends_with(".gz") || lower.ends_with(".tgz") {
        return Some("GZIP archive extension");
    }
    if lower.ends_with(".tar") {
        return Some("TAR archive extension");
    }
    if lower.ends_with(".7z") {
        return Some("7z archive extension");
    }
    if lower.ends_with(".xz") {
        return Some("XZ compressed extension");
    }
    if lower.ends_with(".bz2") {
        return Some("BZIP2 compressed extension");
    }
    if lower.ends_with(".zst") {
        return Some("Zstandard compressed extension");
    }

    if bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
    {
        return Some("ZIP magic header");
    }
    if bytes.starts_with(b"\x1F\x8B") {
        return Some("GZIP magic header");
    }
    if bytes.starts_with(b"\x28\xB5\x2F\xFD") {
        return Some("Zstandard magic header");
    }
    if bytes.starts_with(b"\xFD7zXZ\x00") {
        return Some("XZ magic header");
    }
    if bytes.starts_with(b"7z\xBC\xAF\x27\x1C") {
        return Some("7z magic header");
    }
    if bytes.starts_with(b"BZh") {
        return Some("BZIP2 magic header");
    }
    if let Some(tar_magic) = bytes.get(257..262)
        && tar_magic == b"ustar"
    {
        return Some("POSIX tar ustar header");
    }
    None
}

fn looks_like_local_filesystem_path(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("c:\\")
        || lower.contains("d:\\")
        || lower.contains("\\\\")
        || lower.contains("/users/")
        || lower.contains("/home/")
        || lower.contains("file://")
}

fn validate_safe_external_url(
    url: &str,
    context_id: &str,
    text_bytes_used: &mut usize,
    limits: BundleLimits,
) -> Result<(), BundleError> {
    validate_text_field(url, "reference.external_url", text_bytes_used, limits)?;
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err(BundleError::ForbiddenBundleSideEffect {
            context: context_id.to_owned(),
            reason: "external reference URL must use explicit http:// or https:// scheme",
        });
    }
    let after_scheme = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or("");
    let authority = after_scheme.split('/').next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return Err(BundleError::ForbiddenBundleSideEffect {
            context: context_id.to_owned(),
            reason: "external URL must have a host and must not embed userinfo/credentials",
        });
    }
    let host = authority.split(':').next().unwrap_or("");
    if host == "localhost"
        || host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("169.254.")
        || host.ends_with(".local")
        || host.ends_with(".internal")
    {
        return Err(BundleError::ForbiddenBundleSideEffect {
            context: context_id.to_owned(),
            reason: "external URL must not target loopback, link-local, or private network hosts",
        });
    }
    if lower.contains("token=")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("secret=")
        || lower.contains("password=")
    {
        return Err(BundleError::ForbiddenBundleSideEffect {
            context: context_id.to_owned(),
            reason: "external URL must not include credential or token query parameters",
        });
    }
    Ok(())
}

fn validate_no_side_effect_hooks(
    value: &str,
    context_id: &str,
    text_bytes_used: &mut usize,
    limits: BundleLimits,
) -> Result<(), BundleError> {
    validate_text_field(value, "reproduction.specification", text_bytes_used, limits)?;
    let lower = value.to_ascii_lowercase();
    if lower.contains("exec:")
        || lower.contains("shell:")
        || lower.contains("powershell")
        || lower.contains("/bin/sh")
        || lower.contains("cmd.exe")
        || lower.contains("device://")
        || lower.contains("mavlink://")
        || lower.contains("rtsp://")
        || lower.contains("auto_fetch")
        || lower.contains("api_key=")
        || lower.contains("secret=")
    {
        return Err(BundleError::ForbiddenBundleSideEffect {
            context: context_id.to_owned(),
            reason: "reproduction manifest contains forbidden executable, device, network, or secret hook",
        });
    }
    Ok(())
}

fn validate_pinned_version_refs(
    owner_id: &str,
    refs: &[(&str, &str)],
    seen_record_versions: &BTreeSet<(String, String)>,
    omitted_record_versions: &BTreeMap<(String, String), OmissionReason>,
    text_bytes_used: &mut usize,
    limits: BundleLimits,
) -> Result<(), BundleError> {
    for (rec_id, ver_id) in refs {
        validate_identifier(rec_id, "pinned_input.record_id", text_bytes_used, limits)?;
        validate_identifier(ver_id, "pinned_input.version_id", text_bytes_used, limits)?;
        let key = ((*rec_id).to_owned(), (*ver_id).to_owned());
        if !seen_record_versions.contains(&key) && !omitted_record_versions.contains_key(&key) {
            return Err(BundleError::UnexplainedDanglingReference {
                from_id: owner_id.to_owned(),
                target_id: format!("{rec_id}@{ver_id}"),
            });
        }
    }
    Ok(())
}
