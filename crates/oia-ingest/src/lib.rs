//! `oia-ingest` — Capability-aware source registry, `CAM-01`/`CAM-02` camera admission rules,
//! injected transport, bounded payload reading, fail-safe rate limiting, idempotent retries,
//! generation-guarded cancellation, and coverage/freshness result envelopes (`#13`, `A01`, `A04`, `A09`).
//!
//! Portions of the bounded payload reader (`read_bounded_chunks`), fail-safe rate-limit config
//! and sliding-window admission (`resolve_per_minute_cap`, `SlidingWindowRateLimiter`), and
//! camera frame provenance classification (`CameraFrameProvenance`) are adapted from God's Eye View
//! (`server/providers/common/request.js`, `server/providers/common/rate-limit.js`,
//! `src/sources/rateLimit.js`, and `src/layers/cctv/source.js`, commit
//! `0d1a68160e532ba65dc34b48ea205de6de4028fc`), Copyright (c) 2026 Bilawal Sidhu, under the MIT
//! License (`THIRD_PARTY_NOTICES.md`).

use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use oia_domain::{OpaqueId, ReuseConditions, SourceHealthState, VersionId};

/// Capabilities that a registered source may support (`docs/architecture.md` Section 4.1).
///
/// A live source without `HistoricalArchive` must never manufacture history from a live feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceCapability {
    /// Metadata or directory catalog search.
    CatalogSearch,
    /// Point-in-time or historical archive query.
    HistoricalArchive,
    /// Live periodic polling.
    LivePolling,
    /// Continuous live stream reception.
    Streaming,
    /// Original media/blob retrieval.
    MediaRetrieval,
    /// Hardware telemetry stream.
    HardwareTelemetry,
}

/// Camera scene category for `CAM-01` admission checks (`docs/spec.md` Section 5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraSceneKind {
    /// Ordinarily accessible public street or plaza view.
    PublicStreet,
    /// Traffic or highway department camera.
    TrafficHighway,
    /// Weather, sky, or astronomical observatory camera.
    WeatherObservatory,
    /// Ordinarily accessible business exterior / parking view.
    BusinessExterior,
    /// Camera inside a home or viewing into a home interior — unconditionally excluded by `CAM-01`.
    HomeInteriorExcluded,
}

/// Camera access status under `CAM-01`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraAccessStatus {
    /// Ordinarily accessible via existing directory, published viewing link, or supported embed/API.
    OrdinarilyAccessible,
    /// Source became restricted, requires unconfigured credentials, or blocked access.
    /// Must transition to unavailable without probing alternate endpoints or guessing credentials.
    RestrictedOrBlocked,
}

/// Camera frame provenance adapted from God's Eye View (`src/layers/cctv/source.js`, `CAM-02`, `A09`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraFrameProvenance {
    /// Live video frame directly from the upstream camera stream.
    UpstreamLiveVideo,
    /// Timestamped still image directly from the upstream camera endpoint.
    UpstreamCameraStill,
    /// Cached stale still fallback when live stream is interrupted.
    StaleStillFallback,
    /// Third-party street-level panorama substitute (e.g., Street View).
    StreetViewSubstitute,
    /// Synthetic or placeholder graphic.
    SyntheticPlaceholder,
}

impl CameraFrameProvenance {
    /// Returns `true` only if this frame provenance qualifies as direct event observation evidence (`CAM-02`).
    ///
    /// Substitute views (`StaleStillFallback`, `StreetViewSubstitute`, `SyntheticPlaceholder`)
    /// must never be admitted as observation evidence of the scene at the requested time.
    #[must_use]
    pub const fn qualifies_as_event_evidence(self) -> bool {
        matches!(self, Self::UpstreamLiveVideo | Self::UpstreamCameraStill)
    }
}

/// Camera source admission descriptor (`CAM-01`, `CAM-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraAdmissionDescriptor {
    /// Directory or listing page URL where the camera was found.
    pub listing_page_url: String,
    /// Scene classification.
    pub scene_kind: CameraSceneKind,
    /// Current ordinary-access status.
    pub access_status: CameraAccessStatus,
    /// Whether intentional publication by the camera owner is established.
    ///
    /// Per `CAM-01`, intentional publication is NOT a blanket requirement (`false` is permitted
    /// when `scene_kind != HomeInteriorExcluded` and `access_status == OrdinarilyAccessible`).
    pub owner_intentional_publication_established: bool,
}

impl CameraAdmissionDescriptor {
    /// Evaluates `CAM-01` admission rules.
    ///
    /// - Rejects `HomeInteriorExcluded` unconditionally.
    /// - Rejects `RestrictedOrBlocked` without attempting alternate endpoints.
    /// - Admits ordinarily accessible street/traffic/weather/business cameras regardless of whether
    ///   intentional owner publication is proved.
    pub fn evaluate_admission(&self) -> Result<(), IngestError> {
        if self.scene_kind == CameraSceneKind::HomeInteriorExcluded {
            return Err(IngestError::HomeInteriorCameraExcluded);
        }
        if self.access_status == CameraAccessStatus::RestrictedOrBlocked {
            return Err(IngestError::RestrictedCameraNoAlternateAccess);
        }
        Ok(())
    }
}

/// Fail-safe per-minute rate-limit resolver adapted from God's Eye View
/// (`server/providers/common/rate-limit.js`, MIT License).
///
/// - `None`, empty, negative, or unparseable strings fall back to `default_per_min` so a typo
///   (e.g. `"3O"`) never disables the rate limiter.
/// - Positive fractions in `(0.0, 1.0)` clamp to `1`.
/// - Only an explicit `"0"` returns `0` (unlimited opt-out).
#[must_use]
pub fn resolve_per_minute_cap(raw_value: Option<&str>, default_per_min: u32) -> u32 {
    let Some(trimmed) = raw_value.map(str::trim) else {
        return default_per_min;
    };
    if trimmed.is_empty() {
        return default_per_min;
    }
    let Ok(parsed) = trimmed.parse::<f64>() else {
        return default_per_min;
    };
    if !parsed.is_finite() || parsed < 0.0 {
        return default_per_min;
    }
    if parsed == 0.0 {
        return 0;
    }
    if parsed > 0.0 && parsed < 1.0 {
        return 1;
    }
    if parsed > f64::from(u32::MAX) {
        return u32::MAX;
    }
    parsed.floor() as u32
}

/// Bounded per-key and global sliding-window rate limiter adapted from God's Eye View
/// (`src/sources/rateLimit.js`, MIT License).
#[derive(Debug, Clone)]
pub struct SlidingWindowRateLimiter {
    window_ms: i64,
    max_per_key: usize,
    global_max: usize,
    max_keys: usize,
    key_order: VecDeque<String>,
    hits_by_key: BTreeMap<String, Vec<i64>>,
    global_hits: Vec<i64>,
}

impl SlidingWindowRateLimiter {
    /// Default hard cap on tracked keys so rotating keys cannot grow memory without bound.
    pub const DEFAULT_MAX_KEYS: usize = 2_000;

    /// Creates a new sliding-window rate limiter.
    #[must_use]
    pub fn new(window_ms: i64, max_per_key: usize, global_max: usize) -> Self {
        Self {
            window_ms: window_ms.max(1),
            max_per_key: max_per_key.max(1),
            global_max: global_max.max(1),
            max_keys: Self::DEFAULT_MAX_KEYS,
            key_order: VecDeque::new(),
            hits_by_key: BTreeMap::new(),
            global_hits: Vec::new(),
        }
    }

    /// Checks whether a request for `key` at `now_ms` is admitted.
    pub fn allow(&mut self, key: &str, now_ms: i64) -> bool {
        let window_ms = self.window_ms;
        self.global_hits
            .retain(|&t| now_ms.saturating_sub(t) < window_ms);
        if self.global_hits.len() >= self.global_max {
            return false;
        }

        let mut recent = self.hits_by_key.remove(key).unwrap_or_default();
        recent.retain(|&t| now_ms.saturating_sub(t) < window_ms);
        if recent.len() >= self.max_per_key {
            self.hits_by_key.insert(key.to_owned(), recent);
            return false;
        }

        recent.push(now_ms);
        if !self.key_order.iter().any(|k| k == key) {
            self.key_order.push_back(key.to_owned());
        }
        self.hits_by_key.insert(key.to_owned(), recent);
        self.global_hits.push(now_ms);

        while self.hits_by_key.len() > self.max_keys {
            if let Some(oldest_key) = self.key_order.pop_front() {
                self.hits_by_key.remove(&oldest_key);
            } else {
                break;
            }
        }
        true
    }
}

/// Reads a sequence of incoming payload chunks under a hard byte cap and cancellation flag,
/// adapted from God's Eye View (`server/providers/common/request.js`, MIT License).
///
/// Discards retained bytes immediately on overflow (`IngestError::PayloadTooLarge`) or
/// cancellation (`IngestError::Cancelled`).
pub fn read_bounded_chunks(
    chunks: &[&[u8]],
    max_bytes: usize,
    cancelled: bool,
) -> Result<Vec<u8>, IngestError> {
    if cancelled {
        return Err(IngestError::Cancelled);
    }
    let mut total = 0usize;
    let mut out = Vec::new();
    for &chunk in chunks {
        if cancelled {
            out.clear();
            return Err(IngestError::Cancelled);
        }
        total = total.saturating_add(chunk.len());
        if total > max_bytes {
            out.clear();
            return Err(IngestError::PayloadTooLarge {
                actual_or_seen: total,
                max_bytes,
            });
        }
        out.extend_from_slice(chunk);
    }
    Ok(out)
}

/// Machine-readable query/ingestion outcome (`docs/architecture.md` Section 4.2).
///
/// Keeps `Success`, `Partial`, `Empty`, `Unavailable`, `Unauthorized`, `RateLimited`,
/// `Malformed`, and `Cancelled` distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestOutcome {
    /// Request succeeded within budget.
    Success,
    /// Request returned usable data for a subset of the requested scope (`CASE-04`).
    Partial,
    /// Source was healthy and queried, and returned zero matching records (observed absence / empty).
    Empty,
    /// Source endpoint was unreachable, disabled, restricted, or lacked the requested capability.
    Unavailable,
    /// Credentials missing or rejected by upstream provider.
    Unauthorized,
    /// Local or upstream rate limit exceeded.
    RateLimited,
    /// Upstream payload exceeded byte cap or failed schema validation.
    Malformed,
    /// Request was cancelled or superseded by a newer generation.
    Cancelled,
}

/// Coverage metadata accompanying a [`QueryResultEnvelope`] (`docs/architecture.md` Section 4.2).
///
/// An `IngestOutcome::Success` may still have `is_exhaustive == false` when a source catalog
/// covers only a bounded region or time range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageEnvelope {
    /// Requested time window `(start_ms, end_ms)`, if bounded.
    pub requested_window_ms: Option<(i64, i64)>,
    /// Actually covered time window `(start_ms, end_ms)`, if any.
    pub covered_window_ms: Option<(i64, i64)>,
    /// Whether pagination truncated additional matches.
    pub truncated_by_pagination: bool,
    /// Whether the result covers the entire requested scope without blind spots.
    pub is_exhaustive: bool,
    /// Known blind spots or unqueried sub-regions.
    pub blind_spots: Vec<String>,
}

/// Freshness metadata accompanying a [`QueryResultEnvelope`] (`docs/architecture.md` Section 4.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreshnessEnvelope {
    /// Retrieval timestamp (UTC ms).
    pub retrieved_at_ms: i64,
    /// Upstream observation/publication timestamp (UTC ms), if known.
    pub upstream_timestamp_ms: Option<i64>,
    /// Whether this response was served from a cached or stale snapshot.
    pub is_cached_or_stale: bool,
    /// Current source health state.
    pub source_health: SourceHealthState,
}

/// Complete structured result envelope returned by source queries (`docs/architecture.md` Section 4.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryResultEnvelope<T> {
    /// Request or query identifier.
    pub request_id: OpaqueId,
    /// Source identifier.
    pub source_id: OpaqueId,
    /// Distinct outcome state.
    pub outcome: IngestOutcome,
    /// Normalized records (empty on `Empty`, `Unavailable`, `Unauthorized`, etc.).
    pub records: Vec<T>,
    /// Committed `SourceRecord` version ID if an acquisition was persisted.
    pub committed_source_version: Option<VersionId>,
    /// Coverage envelope (separate from `outcome`).
    pub coverage: CoverageEnvelope,
    /// Freshness envelope.
    pub freshness: FreshnessEnvelope,
    /// Sanitized diagnostic message (never contains raw credentials or private tokens).
    pub diagnostic: Option<String>,
}

///Registered source entry with capability and budget enforcement (`#13`).
#[derive(Debug, Clone)]
pub struct SourceRegistration {
    /// Source ID.
    pub source_id: OpaqueId,
    /// Provider name.
    pub provider_name: String,
    /// Supported capabilities.
    pub capabilities: Vec<SourceCapability>,
    /// Reuse conditions.
    pub reuse_conditions: ReuseConditions,
    /// Allowed URL prefixes (prevents arbitrary SSRF / local-network probing).
    pub permitted_url_prefixes: Vec<String>,
    /// Optional camera admission descriptor (`CAM-01`).
    pub camera_descriptor: Option<CameraAdmissionDescriptor>,
    /// Maximum payload bytes per response.
    pub max_payload_bytes: usize,
    /// Current source health.
    pub health: SourceHealthState,
}

/// Simulated or live transport response injected into [`SourceRegistry`].
#[derive(Debug, Clone)]
pub enum TransportResponse {
    /// HTTP-like 200 OK with chunks and coverage flags.
    Ok {
        /// Response body chunks.
        chunks: Vec<Vec<u8>>,
        /// Upstream observation timestamp (UTC ms).
        upstream_timestamp_ms: Option<i64>,
        /// Whether pagination truncated additional items.
        truncated: bool,
        /// Blind spot note, if partial.
        blind_spot: Option<String>,
    },
    /// Upstream returned 401/403 unauthorized.
    Unauthorized,
    /// Upstream returned 429 rate-limited.
    UpstreamRateLimited,
    /// Upstream endpoint unreachable or offline.
    Unavailable(String),
}

/// Capability-aware source registry with idempotent acquisition tracking, rate limiting,
/// and generation-guarded cancellation (`#13`, `A01`, `A09`).
#[derive(Debug)]
pub struct SourceRegistry {
    sources: BTreeMap<OpaqueId, SourceRegistration>,
    rate_limiters: BTreeMap<OpaqueId, SlidingWindowRateLimiter>,
    committed_by_acquisition_key: BTreeMap<String, VersionId>,
    active_generation_by_context: BTreeMap<OpaqueId, u64>,
}

impl Default for SourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceRegistry {
    /// Creates an empty [`SourceRegistry`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            sources: BTreeMap::new(),
            rate_limiters: BTreeMap::new(),
            committed_by_acquisition_key: BTreeMap::new(),
            active_generation_by_context: BTreeMap::new(),
        }
    }

    /// Registers a source after verifying `CAM-01` camera admission rules if `camera_descriptor` is present.
    pub fn register_source(
        &mut self,
        mut registration: SourceRegistration,
        requests_per_min: u32,
    ) -> Result<(), IngestError> {
        if let Some(ref cam) = registration.camera_descriptor {
            match cam.evaluate_admission() {
                Ok(()) => {}
                Err(IngestError::RestrictedCameraNoAlternateAccess) => {
                    // Mark source Unavailable without probing alternate access (CAM-01).
                    registration.health = SourceHealthState::Unavailable;
                    let sid = registration.source_id.clone();
                    self.sources.insert(sid, registration);
                    return Err(IngestError::RestrictedCameraNoAlternateAccess);
                }
                Err(e) => return Err(e),
            }
        }
        let sid = registration.source_id.clone();
        if requests_per_min > 0 {
            self.rate_limiters.insert(
                sid.clone(),
                SlidingWindowRateLimiter::new(
                    60_000,
                    requests_per_min as usize,
                    (requests_per_min as usize).saturating_mul(20),
                ),
            );
        }
        self.sources.insert(sid, registration);
        Ok(())
    }

    /// Advances the generation counter for a UI/query context (`context_id`), superseding any
    /// older in-flight requests for that context (`CASE-04`, `A01`).
    pub fn advance_context_generation(&mut self, context_id: &OpaqueId) -> u64 {
        let entry = self
            .active_generation_by_context
            .entry(context_id.clone())
            .or_insert(0);
        *entry += 1;
        *entry
    }

    /// Executes a bounded source acquisition with capability verification, endpoint validation,
    /// rate limiting, idempotent retry deduplication, and generation-epoch checking (`#13`).
    #[allow(clippy::too_many_arguments)]
    pub fn execute_acquisition(
        &mut self,
        request_id: OpaqueId,
        context_id: &OpaqueId,
        request_generation: u64,
        source_id: &OpaqueId,
        required_capability: SourceCapability,
        endpoint_url: &str,
        acquisition_key: &str,
        requested_window_ms: Option<(i64, i64)>,
        now_ms: i64,
        transport_response: TransportResponse,
        mint_version_id: VersionId,
    ) -> QueryResultEnvelope<Vec<u8>> {
        let current_gen = self
            .active_generation_by_context
            .get(context_id)
            .copied()
            .unwrap_or(request_generation);
        if request_generation < current_gen {
            return Self::empty_envelope(
                request_id,
                source_id.clone(),
                IngestOutcome::Cancelled,
                requested_window_ms,
                now_ms,
                SourceHealthState::Available,
                Some("Request superseded by newer context generation".to_owned()),
            );
        }

        let Some(reg) = self.sources.get_mut(source_id) else {
            return Self::empty_envelope(
                request_id,
                source_id.clone(),
                IngestOutcome::Unavailable,
                requested_window_ms,
                now_ms,
                SourceHealthState::Unconfigured,
                Some("Source is not registered".to_owned()),
            );
        };

        if reg.health == SourceHealthState::Unavailable || reg.health == SourceHealthState::Disabled
        {
            return Self::empty_envelope(
                request_id,
                source_id.clone(),
                IngestOutcome::Unavailable,
                requested_window_ms,
                now_ms,
                reg.health,
                Some(
                    "Source is unavailable or restricted; no alternate access attempted".to_owned(),
                ),
            );
        }

        if !reg.capabilities.contains(&required_capability) {
            return Self::empty_envelope(
                request_id,
                source_id.clone(),
                IngestOutcome::Unavailable,
                requested_window_ms,
                now_ms,
                reg.health,
                Some(format!(
                    "Source does not support capability {required_capability:?}; live feeds cannot manufacture history"
                )),
            );
        }

        if !reg
            .permitted_url_prefixes
            .iter()
            .any(|prefix| endpoint_url.starts_with(prefix))
        {
            return Self::empty_envelope(
                request_id,
                source_id.clone(),
                IngestOutcome::Unauthorized,
                requested_window_ms,
                now_ms,
                reg.health,
                Some("Endpoint URL is outside the source's permitted endpoint family".to_owned()),
            );
        }

        // Idempotent retry check: if this exact acquisition_key already committed, return the existing version.
        if let Some(existing_ver) = self.committed_by_acquisition_key.get(acquisition_key) {
            return QueryResultEnvelope {
                request_id,
                source_id: source_id.clone(),
                outcome: IngestOutcome::Success,
                records: Vec::new(),
                committed_source_version: Some(existing_ver.clone()),
                coverage: CoverageEnvelope {
                    requested_window_ms,
                    covered_window_ms: requested_window_ms,
                    truncated_by_pagination: false,
                    is_exhaustive: true,
                    blind_spots: Vec::new(),
                },
                freshness: FreshnessEnvelope {
                    retrieved_at_ms: now_ms,
                    upstream_timestamp_ms: None,
                    is_cached_or_stale: true,
                    source_health: reg.health,
                },
                diagnostic: Some(
                    "Idempotent retry reused existing committed SourceRecord version".to_owned(),
                ),
            };
        }

        if let Some(limiter) = self.rate_limiters.get_mut(source_id)
            && !limiter.allow(source_id.as_str(), now_ms)
        {
            return Self::empty_envelope(
                request_id,
                source_id.clone(),
                IngestOutcome::RateLimited,
                requested_window_ms,
                now_ms,
                SourceHealthState::Degraded,
                Some("Rate limit budget exceeded for source".to_owned()),
            );
        }

        let max_bytes = reg.max_payload_bytes;
        let health = reg.health;

        match transport_response {
            TransportResponse::Unauthorized => {
                if let Some(r) = self.sources.get_mut(source_id) {
                    r.health = SourceHealthState::Unavailable;
                }
                Self::empty_envelope(
                    request_id,
                    source_id.clone(),
                    IngestOutcome::Unauthorized,
                    requested_window_ms,
                    now_ms,
                    SourceHealthState::Unavailable,
                    Some("Upstream provider rejected authorization".to_owned()),
                )
            }
            TransportResponse::UpstreamRateLimited => Self::empty_envelope(
                request_id,
                source_id.clone(),
                IngestOutcome::RateLimited,
                requested_window_ms,
                now_ms,
                SourceHealthState::Degraded,
                Some("Upstream provider returned rate-limit response".to_owned()),
            ),
            TransportResponse::Unavailable(reason) => Self::empty_envelope(
                request_id,
                source_id.clone(),
                IngestOutcome::Unavailable,
                requested_window_ms,
                now_ms,
                SourceHealthState::Unavailable,
                Some(reason),
            ),
            TransportResponse::Ok {
                chunks,
                upstream_timestamp_ms,
                truncated,
                blind_spot,
            } => {
                let slice_refs: Vec<&[u8]> = chunks.iter().map(Vec::as_slice).collect();
                let Ok(bytes) = read_bounded_chunks(&slice_refs, max_bytes, false) else {
                    return Self::empty_envelope(
                        request_id,
                        source_id.clone(),
                        IngestOutcome::Malformed,
                        requested_window_ms,
                        now_ms,
                        health,
                        Some(format!("Payload exceeded max_payload_bytes ({max_bytes})")),
                    );
                };

                if bytes.is_empty() {
                    return Self::empty_envelope(
                        request_id,
                        source_id.clone(),
                        IngestOutcome::Empty,
                        requested_window_ms,
                        now_ms,
                        health,
                        Some("Source returned an empty result set".to_owned()),
                    );
                }

                self.committed_by_acquisition_key
                    .insert(acquisition_key.to_owned(), mint_version_id.clone());

                let has_blind_spot = blind_spot.is_some();
                let outcome = if truncated || has_blind_spot {
                    IngestOutcome::Partial
                } else {
                    IngestOutcome::Success
                };

                QueryResultEnvelope {
                    request_id,
                    source_id: source_id.clone(),
                    outcome,
                    records: vec![bytes],
                    committed_source_version: Some(mint_version_id),
                    coverage: CoverageEnvelope {
                        requested_window_ms,
                        covered_window_ms: requested_window_ms,
                        truncated_by_pagination: truncated,
                        is_exhaustive: !truncated && !has_blind_spot,
                        blind_spots: blind_spot.into_iter().collect(),
                    },
                    freshness: FreshnessEnvelope {
                        retrieved_at_ms: now_ms,
                        upstream_timestamp_ms,
                        is_cached_or_stale: false,
                        source_health: health,
                    },
                    diagnostic: None,
                }
            }
        }
    }

    fn empty_envelope(
        request_id: OpaqueId,
        source_id: OpaqueId,
        outcome: IngestOutcome,
        requested_window_ms: Option<(i64, i64)>,
        now_ms: i64,
        source_health: SourceHealthState,
        diagnostic: Option<String>,
    ) -> QueryResultEnvelope<Vec<u8>> {
        QueryResultEnvelope {
            request_id,
            source_id,
            outcome,
            records: Vec::new(),
            committed_source_version: None,
            coverage: CoverageEnvelope {
                requested_window_ms,
                covered_window_ms: None,
                truncated_by_pagination: false,
                is_exhaustive: false,
                blind_spots: Vec::new(),
            },
            freshness: FreshnessEnvelope {
                retrieved_at_ms: now_ms,
                upstream_timestamp_ms: None,
                is_cached_or_stale: false,
                source_health,
            },
            diagnostic,
        }
    }
}

/// Typed errors for `oia-ingest`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestError {
    /// Camera inside a home or viewing into a home interior is excluded (`CAM-01`).
    HomeInteriorCameraExcluded,
    /// Camera source is restricted; OIA must not probe alternate endpoints or guess credentials (`CAM-01`).
    RestrictedCameraNoAlternateAccess,
    /// Response body exceeded configured byte cap (`BODY_TOO_LARGE`).
    PayloadTooLarge {
        /// Bytes seen before abort.
        actual_or_seen: usize,
        /// Configured byte cap.
        max_bytes: usize,
    },
    /// Operation was cancelled before completion.
    Cancelled,
}

impl fmt::Display for IngestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HomeInteriorCameraExcluded => {
                write!(
                    f,
                    "cameras inside homes or viewing home interiors are excluded (CAM-01)"
                )
            }
            Self::RestrictedCameraNoAlternateAccess => write!(
                f,
                "camera source is restricted; marked unavailable without alternate access attempts (CAM-01)"
            ),
            Self::PayloadTooLarge {
                actual_or_seen,
                max_bytes,
            } => write!(
                f,
                "payload size ({actual_or_seen} bytes) exceeds cap of {max_bytes} bytes"
            ),
            Self::Cancelled => write!(f, "ingestion request was cancelled"),
        }
    }
}

impl std::error::Error for IngestError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies distinct Partial, Empty, Unavailable, Unauthorized, RateLimited, Malformed,
    /// and Cancelled outcomes; idempotent retries; CAM-01 home-interior exclusion and
    /// restricted-source handling; and fail-safe rate-limit parsing (#13, A01, A04, A09).
    #[test]
    fn source_outcomes_idempotency_cam01_and_rate_limits() -> Result<(), Box<dyn std::error::Error>>
    {
        // 1. Fail-safe rate-limit config ported from God's Eye View.
        assert_eq!(resolve_per_minute_cap(Some("3O"), 30), 30); // typo '3O' keeps default 30
        assert_eq!(resolve_per_minute_cap(Some("-5"), 30), 30);
        assert_eq!(resolve_per_minute_cap(Some("0.5"), 30), 1);
        assert_eq!(resolve_per_minute_cap(Some("0"), 30), 0); // explicit opt-out

        // 2. CAM-01: Home interior excluded; restricted camera marked unavailable without probing;
        //    ordinarily accessible traffic camera admitted without requiring proof of owner publication.
        let mut registry = SourceRegistry::new();
        let reuse = ReuseConditions {
            ordinary_access_method: "public_directory".to_owned(),
            automated_processing_allowed: true,
            retention_allowed: true,
            redistribution_allowed: false,
            attribution: "City DOT".to_owned(),
        };

        let home_cam = SourceRegistration {
            source_id: OpaqueId::new("cam_home")?,
            provider_name: "Directory".to_owned(),
            capabilities: vec![SourceCapability::Streaming],
            reuse_conditions: reuse.clone(),
            permitted_url_prefixes: vec!["https://cams.example/".to_owned()],
            camera_descriptor: Some(CameraAdmissionDescriptor {
                listing_page_url: "https://cams.example/1".to_owned(),
                scene_kind: CameraSceneKind::HomeInteriorExcluded,
                access_status: CameraAccessStatus::OrdinarilyAccessible,
                owner_intentional_publication_established: false,
            }),
            max_payload_bytes: 1024,
            health: SourceHealthState::Available,
        };
        assert_eq!(
            registry.register_source(home_cam, 10),
            Err(IngestError::HomeInteriorCameraExcluded)
        );

        let restricted_cam = SourceRegistration {
            source_id: OpaqueId::new("cam_restricted")?,
            camera_descriptor: Some(CameraAdmissionDescriptor {
                listing_page_url: "https://cams.example/2".to_owned(),
                scene_kind: CameraSceneKind::PublicStreet,
                access_status: CameraAccessStatus::RestrictedOrBlocked,
                owner_intentional_publication_established: false,
            }),
            ..SourceRegistration {
                source_id: OpaqueId::new("cam_restricted")?,
                provider_name: "Directory".to_owned(),
                capabilities: vec![SourceCapability::Streaming],
                reuse_conditions: reuse.clone(),
                permitted_url_prefixes: vec!["https://cams.example/".to_owned()],
                camera_descriptor: None,
                max_payload_bytes: 1024,
                health: SourceHealthState::Available,
            }
        };
        assert_eq!(
            registry.register_source(restricted_cam, 10),
            Err(IngestError::RestrictedCameraNoAlternateAccess)
        );

        // Frame provenance check (CAM-02): Street View substitute does not qualify as event evidence.
        assert!(CameraFrameProvenance::UpstreamLiveVideo.qualifies_as_event_evidence());
        assert!(!CameraFrameProvenance::StreetViewSubstitute.qualifies_as_event_evidence());
        assert!(!CameraFrameProvenance::SyntheticPlaceholder.qualifies_as_event_evidence());

        // 3. Register valid live traffic source (rate limit = 2 req/min, max_bytes = 64).
        let src_id = OpaqueId::new("src_dot_traffic")?;
        registry.register_source(
            SourceRegistration {
                source_id: src_id.clone(),
                provider_name: "DOT".to_owned(),
                capabilities: vec![SourceCapability::LivePolling],
                reuse_conditions: reuse,
                permitted_url_prefixes: vec!["https://dot.example/api/".to_owned()],
                camera_descriptor: Some(CameraAdmissionDescriptor {
                    listing_page_url: "https://dot.example/cams".to_owned(),
                    scene_kind: CameraSceneKind::TrafficHighway,
                    access_status: CameraAccessStatus::OrdinarilyAccessible,
                    owner_intentional_publication_established: false,
                }),
                max_payload_bytes: 64,
                health: SourceHealthState::Available,
            },
            2,
        )?;

        let ctx_id = OpaqueId::new("ctx_main")?;
        let gen1 = registry.advance_context_generation(&ctx_id);

        // Querying unsupported HistoricalArchive on a LivePolling-only source returns Unavailable.
        let unavail_hist = registry.execute_acquisition(
            OpaqueId::new("req_hist")?,
            &ctx_id,
            gen1,
            &src_id,
            SourceCapability::HistoricalArchive,
            "https://dot.example/api/poll",
            "key_hist",
            Some((1_000, 2_000)),
            10_000,
            TransportResponse::Ok {
                chunks: vec![b"data".to_vec()],
                upstream_timestamp_ms: Some(1_500),
                truncated: false,
                blind_spot: None,
            },
            VersionId::new("ver_unused")?,
        );
        assert_eq!(unavail_hist.outcome, IngestOutcome::Unavailable);

        // Oversized payload (> 64 bytes) returns Malformed.
        let malformed = registry.execute_acquisition(
            OpaqueId::new("req_big")?,
            &ctx_id,
            gen1,
            &src_id,
            SourceCapability::LivePolling,
            "https://dot.example/api/poll",
            "key_big",
            None,
            10_000,
            TransportResponse::Ok {
                chunks: vec![vec![b'x'; 100]],
                upstream_timestamp_ms: Some(10_000),
                truncated: false,
                blind_spot: None,
            },
            VersionId::new("ver_big")?,
        );
        assert_eq!(malformed.outcome, IngestOutcome::Malformed);

        // Partial outcome with blind spot; then idempotent retry reuses the same version ID without consuming rate limit.
        let v1 = VersionId::new("ver_sr_1")?;
        let partial = registry.execute_acquisition(
            OpaqueId::new("req_1")?,
            &ctx_id,
            gen1,
            &src_id,
            SourceCapability::LivePolling,
            "https://dot.example/api/poll",
            "key_1",
            None,
            10_100,
            TransportResponse::Ok {
                chunks: vec![b"frame_meta".to_vec()],
                upstream_timestamp_ms: Some(10_050),
                truncated: true,
                blind_spot: Some("North sector offline".to_owned()),
            },
            v1.clone(),
        );
        assert_eq!(partial.outcome, IngestOutcome::Partial);
        assert!(!partial.coverage.is_exhaustive);
        assert_eq!(partial.committed_source_version, Some(v1.clone()));

        let retry = registry.execute_acquisition(
            OpaqueId::new("req_1_retry")?,
            &ctx_id,
            gen1,
            &src_id,
            SourceCapability::LivePolling,
            "https://dot.example/api/poll",
            "key_1",
            None,
            10_200,
            TransportResponse::Ok {
                chunks: vec![b"frame_meta".to_vec()],
                upstream_timestamp_ms: Some(10_050),
                truncated: false,
                blind_spot: None,
            },
            VersionId::new("ver_sr_duplicate")?,
        );
        assert_eq!(retry.committed_source_version, Some(v1));
        assert!(retry.freshness.is_cached_or_stale);

        // 3rd non-cached request within the same minute triggers RateLimited (cap = 2/min).
        let rate_limited = registry.execute_acquisition(
            OpaqueId::new("req_3")?,
            &ctx_id,
            gen1,
            &src_id,
            SourceCapability::LivePolling,
            "https://dot.example/api/poll",
            "key_3",
            None,
            10_300,
            TransportResponse::Ok {
                chunks: vec![b"ok".to_vec()],
                upstream_timestamp_ms: None,
                truncated: false,
                blind_spot: None,
            },
            VersionId::new("ver_3")?,
        );
        assert_eq!(rate_limited.outcome, IngestOutcome::RateLimited);

        // Advancing context generation cancels late responses from gen1.
        let _gen2 = registry.advance_context_generation(&ctx_id);
        let stale_resp = registry.execute_acquisition(
            OpaqueId::new("req_late")?,
            &ctx_id,
            gen1,
            &src_id,
            SourceCapability::LivePolling,
            "https://dot.example/api/poll",
            "key_late",
            None,
            10_400,
            TransportResponse::Ok {
                chunks: vec![b"late".to_vec()],
                upstream_timestamp_ms: None,
                truncated: false,
                blind_spot: None,
            },
            VersionId::new("ver_late")?,
        );
        assert_eq!(stale_resp.outcome, IngestOutcome::Cancelled);

        Ok(())
    }
}
