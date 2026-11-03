//! Deterministic quote/release selection and split-only adjustment at explicit cutoffs.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use oia_domain::{OpaqueId, VersionId};

use crate::model::{
    CorporateActionRecord, CutoffMode, ExactRational, ExcludedVersion, ExclusionReason,
    PriceAdjustmentBasis, QuoteDelay, QuoteRecord, ReleasePayload, ReleaseVintage, Unit,
    availability_exclusion, validate_availability, validate_cutoff_mode,
};
use crate::{MarketError, MarketLimits, StopReason, validate_timestamp};

pub const UNADJUSTED_METHOD_ID: &str = "unadjusted-exact-rational-v1";
pub const SPLIT_ADJUSTED_METHOD_ID: &str = "split-adjusted-exact-rational-v1";

/// Caller-selected quote scope. Records from another scope are errors, not candidates.
#[derive(Debug, Clone, Copy)]
pub struct QuoteQuery<'a> {
    pub instrument_id: &'a OpaqueId,
    pub venue: &'a OpaqueId,
    pub source_id: &'a OpaqueId,
    pub unit: &'a Unit,
    pub cutoff_utc_ms: i64,
    pub mode: &'a CutoffMode,
}

#[derive(Debug, Clone)]
pub struct QuoteSelection<'a> {
    selected: Option<&'a QuoteRecord>,
    supplied_count: usize,
    eligible_count: usize,
    exclusions: Vec<ExcludedVersion>,
    cutoff_utc_ms: i64,
    mode: CutoffMode,
}

impl<'a> QuoteSelection<'a> {
    #[must_use]
    pub fn selected(&self) -> Option<&'a QuoteRecord> {
        self.selected
    }
    #[must_use]
    pub fn supplied_count(&self) -> usize {
        self.supplied_count
    }
    #[must_use]
    pub fn eligible_count(&self) -> usize {
        self.eligible_count
    }
    #[must_use]
    pub fn exclusions(&self) -> &[ExcludedVersion] {
        &self.exclusions
    }
    #[must_use]
    pub fn cutoff_utc_ms(&self) -> i64 {
        self.cutoff_utc_ms
    }
    #[must_use]
    pub fn mode(&self) -> &CutoffMode {
        &self.mode
    }
}

fn validate_quote(quote: &QuoteRecord) -> Result<(), MarketError> {
    validate_timestamp("quote observed_at_utc_ms", quote.observed_at_utc_ms)?;
    validate_availability(quote.availability)?;
    if quote
        .availability
        .public_available_at_utc_ms
        .is_some_and(|public| quote.observed_at_utc_ms > public)
    {
        return Err(MarketError::InvalidQuote(
            "observation cannot follow documented public availability",
        ));
    }
    if matches!(
        quote.delay,
        QuoteDelay::Delayed {
            declared_delay_ms: Some(0)
        }
    ) {
        return Err(MarketError::InvalidQuote(
            "a declared zero delay must use RealTime",
        ));
    }
    Ok(())
}

/// Selects the latest admissible quote in one exact instrument/venue/source/unit scope.
/// Empty input, an empty eligible set, and excluded versions remain distinguishable.
pub fn select_quote_as_of<'a>(
    quotes: &'a [QuoteRecord],
    query: QuoteQuery<'_>,
    limits: MarketLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<QuoteSelection<'a>, MarketError> {
    check_control().map_err(MarketError::Stopped)?;
    limits.check_len(quotes.len())?;
    validate_timestamp("quote cutoff_utc_ms", query.cutoff_utc_ms)?;
    validate_cutoff_mode(query.mode)?;
    let mut versions = BTreeSet::new();
    let mut exclusions = Vec::new();
    exclusions
        .try_reserve_exact(quotes.len())
        .map_err(|_| MarketError::AllocationFailed)?;
    let mut selected: Option<&QuoteRecord> = None;
    let mut eligible_count = 0_usize;
    for quote in quotes {
        check_control().map_err(MarketError::Stopped)?;
        validate_quote(quote)?;
        if &quote.instrument_id != query.instrument_id {
            return Err(MarketError::ScopeMismatch("quote instrument_id"));
        }
        if &quote.venue != query.venue {
            return Err(MarketError::ScopeMismatch("quote venue"));
        }
        if &quote.source_id != query.source_id {
            return Err(MarketError::ScopeMismatch("quote source_id"));
        }
        if &quote.unit != query.unit {
            return Err(MarketError::ScopeMismatch("quote unit"));
        }
        if !versions.insert(&quote.version) {
            return Err(MarketError::DuplicateVersion(quote.version.clone()));
        }
        let exclusion = if quote.observed_at_utc_ms > query.cutoff_utc_ms {
            Some(ExclusionReason::ObservationAfterCutoff)
        } else {
            availability_exclusion(quote.availability, query.cutoff_utc_ms, query.mode)
        };
        if let Some(reason) = exclusion {
            exclusions.push(ExcludedVersion {
                version: quote.version.clone(),
                reason,
            });
            continue;
        }
        eligible_count += 1;
        let replace = match selected {
            None => true,
            Some(current) => {
                let ordering = quote
                    .observed_at_utc_ms
                    .cmp(&current.observed_at_utc_ms)
                    .then_with(|| {
                        quote
                            .availability
                            .public_available_at_utc_ms
                            .cmp(&current.availability.public_available_at_utc_ms)
                    })
                    .then_with(|| quote.version.cmp(&current.version));
                ordering == Ordering::Greater
            }
        };
        if replace {
            selected = Some(quote);
        }
    }
    check_control().map_err(MarketError::Stopped)?;
    Ok(QuoteSelection {
        selected,
        supplied_count: quotes.len(),
        eligible_count,
        exclusions,
        cutoff_utc_ms: query.cutoff_utc_ms,
        mode: query.mode.clone(),
    })
}

#[derive(Debug, Clone)]
pub struct ReleaseSelection<'a> {
    selected: Option<&'a ReleaseVintage>,
    supplied_count: usize,
    eligible_count: usize,
    exclusions: Vec<ExcludedVersion>,
    cutoff_utc_ms: i64,
    mode: CutoffMode,
}

impl<'a> ReleaseSelection<'a> {
    #[must_use]
    pub fn selected(&self) -> Option<&'a ReleaseVintage> {
        self.selected
    }
    #[must_use]
    pub fn supplied_count(&self) -> usize {
        self.supplied_count
    }
    #[must_use]
    pub fn eligible_count(&self) -> usize {
        self.eligible_count
    }
    #[must_use]
    pub fn exclusions(&self) -> &[ExcludedVersion] {
        &self.exclusions
    }
    #[must_use]
    pub fn cutoff_utc_ms(&self) -> i64 {
        self.cutoff_utc_ms
    }
    #[must_use]
    pub fn mode(&self) -> &CutoffMode {
        &self.mode
    }
}

/// Selects the highest revision admissible at the cutoff, not the newest revision
/// currently present in the input. Reporting/event dates do not prove publication.
pub fn select_release_vintage<'a>(
    vintages: &'a [ReleaseVintage],
    series_id: &OpaqueId,
    cutoff_utc_ms: i64,
    mode: &CutoffMode,
    limits: MarketLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<ReleaseSelection<'a>, MarketError> {
    check_control().map_err(MarketError::Stopped)?;
    limits.check_len(vintages.len())?;
    validate_timestamp("release cutoff_utc_ms", cutoff_utc_ms)?;
    validate_cutoff_mode(mode)?;
    let mut versions = BTreeSet::new();
    let mut revisions = BTreeMap::new();
    let mut expected_unit = None;
    let mut expected_source = None;
    let mut expected_period = None;
    let mut payload_kind = None;
    for vintage in vintages {
        check_control().map_err(MarketError::Stopped)?;
        if &vintage.series_id != series_id {
            return Err(MarketError::ScopeMismatch("release series_id"));
        }
        if vintage.revision == 0 {
            return Err(MarketError::InvalidRelease(
                "revision sequence starts at one",
            ));
        }
        if vintage.reporting_period.start >= vintage.reporting_period.end_exclusive {
            return Err(MarketError::InvalidRelease("invalid reporting period"));
        }
        if expected_source.is_some_and(|source| source != &vintage.source_id) {
            return Err(MarketError::InvalidRelease(
                "source identity cannot change within a revision series",
            ));
        }
        expected_source = Some(&vintage.source_id);
        if expected_period.is_some_and(|period| period != vintage.reporting_period) {
            return Err(MarketError::InvalidRelease(
                "reporting period cannot change within a revision series",
            ));
        }
        expected_period = Some(vintage.reporting_period);
        if let Some(event) = vintage.event_at_utc_ms {
            validate_timestamp("release event_at_utc_ms", event)?;
        }
        validate_availability(vintage.availability)?;
        if !versions.insert(&vintage.version) {
            return Err(MarketError::DuplicateVersion(vintage.version.clone()));
        }
        if revisions.insert(vintage.revision, vintage).is_some() {
            return Err(MarketError::DuplicateRevision(vintage.revision));
        }
        match &vintage.payload {
            ReleasePayload::Numeric { unit, .. } => {
                if payload_kind == Some(false) {
                    return Err(MarketError::InvalidRelease(
                        "numeric and disclosure vintages cannot share one series",
                    ));
                }
                payload_kind = Some(true);
                if let Some(expected) = expected_unit {
                    if expected != unit {
                        return Err(MarketError::UnitMismatch);
                    }
                } else {
                    expected_unit = Some(unit);
                }
            }
            ReleasePayload::Disclosure {
                linked_entities, ..
            } => {
                if payload_kind == Some(true) {
                    return Err(MarketError::InvalidRelease(
                        "numeric and disclosure vintages cannot share one series",
                    ));
                }
                payload_kind = Some(false);
                limits.check_len(linked_entities.len())?;
                let mut entities = BTreeSet::new();
                for entity in linked_entities {
                    if !entities.insert(entity) {
                        return Err(MarketError::InvalidRelease(
                            "duplicate linked entity in disclosure",
                        ));
                    }
                }
            }
        }
    }
    for (revision, vintage) in &revisions {
        check_control().map_err(MarketError::Stopped)?;
        match (&vintage.predecessor_version, revisions.get(&(revision - 1))) {
            (None, _) if *revision == 1 => {}
            (Some(predecessor), Some(previous)) if *revision > 1 => {
                if predecessor != &previous.version {
                    return Err(MarketError::InvalidRelease(
                        "predecessor does not match the previous revision",
                    ));
                }
                if let (Some(current_public), Some(previous_public)) = (
                    vintage.availability.public_available_at_utc_ms,
                    previous.availability.public_available_at_utc_ms,
                ) && current_public < previous_public
                {
                    return Err(MarketError::InvalidRelease(
                        "public availability moves backward across revisions",
                    ));
                }
            }
            _ => {
                return Err(MarketError::InvalidRelease(
                    "revision chain must be consecutive and explicitly linked",
                ));
            }
        }
    }
    let mut exclusions = Vec::new();
    exclusions
        .try_reserve_exact(vintages.len())
        .map_err(|_| MarketError::AllocationFailed)?;
    let mut selected = None;
    let mut eligible_count = 0_usize;
    for vintage in vintages {
        check_control().map_err(MarketError::Stopped)?;
        if let Some(reason) = availability_exclusion(vintage.availability, cutoff_utc_ms, mode) {
            exclusions.push(ExcludedVersion {
                version: vintage.version.clone(),
                reason,
            });
            continue;
        }
        eligible_count += 1;
        if selected.is_none_or(|current: &ReleaseVintage| vintage.revision > current.revision) {
            selected = Some(vintage);
        }
    }
    check_control().map_err(MarketError::Stopped)?;
    Ok(ReleaseSelection {
        selected,
        supplied_count: vintages.len(),
        eligible_count,
        exclusions,
        cutoff_utc_ms,
        mode: mode.clone(),
    })
}

#[derive(Debug, Clone)]
pub struct AdjustedQuote<'a> {
    original: &'a QuoteRecord,
    adjusted_price: ExactRational,
    basis: PriceAdjustmentBasis,
    method_id: &'static str,
    applied_actions: Vec<VersionId>,
    excluded_actions: Vec<ExcludedVersion>,
    cutoff_utc_ms: i64,
    mode: CutoffMode,
}

impl<'a> AdjustedQuote<'a> {
    #[must_use]
    pub fn original(&self) -> &'a QuoteRecord {
        self.original
    }
    #[must_use]
    pub fn adjusted_price(&self) -> ExactRational {
        self.adjusted_price
    }
    #[must_use]
    pub fn basis(&self) -> PriceAdjustmentBasis {
        self.basis
    }
    #[must_use]
    pub fn method_id(&self) -> &str {
        self.method_id
    }
    #[must_use]
    pub fn applied_actions(&self) -> &[VersionId] {
        &self.applied_actions
    }
    #[must_use]
    pub fn excluded_actions(&self) -> &[ExcludedVersion] {
        &self.excluded_actions
    }
    #[must_use]
    pub fn cutoff_utc_ms(&self) -> i64 {
        self.cutoff_utc_ms
    }
    #[must_use]
    pub fn mode(&self) -> &CutoffMode {
        &self.mode
    }
}

fn validate_action(action: &CorporateActionRecord) -> Result<(), MarketError> {
    validate_timestamp("action effective_at_utc_ms", action.effective_at_utc_ms)?;
    validate_availability(action.availability)?;
    if action.split_numerator <= 0 || action.split_denominator <= 0 {
        return Err(MarketError::InvalidAction(
            "split ratio terms must be positive",
        ));
    }
    Ok(())
}

/// Expresses a quote on an unadjusted or post-split share basis. Only actions
/// effective after the observation and admissible at the cutoff are applied.
/// Total-return, dividend, FX, and other adjustments are deliberately unsupported.
pub fn adjust_quote<'a>(
    quote: &'a QuoteRecord,
    actions: &[CorporateActionRecord],
    basis: PriceAdjustmentBasis,
    cutoff_utc_ms: i64,
    mode: &CutoffMode,
    limits: MarketLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<AdjustedQuote<'a>, MarketError> {
    check_control().map_err(MarketError::Stopped)?;
    validate_timestamp("adjustment cutoff_utc_ms", cutoff_utc_ms)?;
    validate_cutoff_mode(mode)?;
    validate_quote(quote)?;
    if basis == PriceAdjustmentBasis::TotalReturn {
        return Err(MarketError::UnsupportedAdjustment);
    }
    let quote_exclusion = if quote.observed_at_utc_ms > cutoff_utc_ms {
        Some(ExclusionReason::ObservationAfterCutoff)
    } else {
        availability_exclusion(quote.availability, cutoff_utc_ms, mode)
    };
    if let Some(reason) = quote_exclusion {
        return Err(MarketError::QuoteExcluded {
            version: quote.version.clone(),
            reason,
        });
    }
    limits.check_len(actions.len())?;
    let mut adjusted_price = ExactRational::from_decimal(quote.price)?;
    let method_id = if basis == PriceAdjustmentBasis::SplitAdjusted {
        SPLIT_ADJUSTED_METHOD_ID
    } else {
        UNADJUSTED_METHOD_ID
    };
    let mut applied_actions = Vec::new();
    let mut excluded_actions = Vec::new();
    if basis == PriceAdjustmentBasis::SplitAdjusted {
        applied_actions
            .try_reserve_exact(actions.len())
            .map_err(|_| MarketError::AllocationFailed)?;
        excluded_actions
            .try_reserve_exact(actions.len())
            .map_err(|_| MarketError::AllocationFailed)?;
        let mut eligible = Vec::new();
        eligible
            .try_reserve_exact(actions.len())
            .map_err(|_| MarketError::AllocationFailed)?;
        let mut versions = BTreeSet::new();
        let mut effective_instants = BTreeSet::new();
        for action in actions {
            check_control().map_err(MarketError::Stopped)?;
            validate_action(action)?;
            if action.instrument_id != quote.instrument_id {
                return Err(MarketError::ScopeMismatch("action instrument_id"));
            }
            if !versions.insert(&action.version) {
                return Err(MarketError::DuplicateVersion(action.version.clone()));
            }
            if !effective_instants.insert(action.effective_at_utc_ms) {
                return Err(MarketError::InvalidAction(
                    "multiple split actions at one instrument/effective instant are ambiguous",
                ));
            }
            let exclusion = if action.effective_at_utc_ms > cutoff_utc_ms {
                Some(ExclusionReason::EffectiveAfterCutoff)
            } else if let Some(reason) =
                availability_exclusion(action.availability, cutoff_utc_ms, mode)
            {
                Some(reason)
            } else if action.effective_at_utc_ms <= quote.observed_at_utc_ms {
                Some(ExclusionReason::NotEffectiveForQuote)
            } else {
                None
            };
            if let Some(reason) = exclusion {
                excluded_actions.push(ExcludedVersion {
                    version: action.version.clone(),
                    reason,
                });
            } else {
                eligible.push(action);
            }
        }
        eligible.sort_by(|left, right| {
            left.effective_at_utc_ms
                .cmp(&right.effective_at_utc_ms)
                .then_with(|| left.version.cmp(&right.version))
        });
        for action in eligible {
            check_control().map_err(MarketError::Stopped)?;
            adjusted_price = adjusted_price
                .checked_multiply(action.split_denominator, action.split_numerator)?;
            applied_actions.push(action.version.clone());
        }
    }
    check_control().map_err(MarketError::Stopped)?;
    Ok(AdjustedQuote {
        original: quote,
        adjusted_price,
        basis,
        method_id,
        applied_actions,
        excluded_actions,
        cutoff_utc_ms,
        mode: mode.clone(),
    })
}
