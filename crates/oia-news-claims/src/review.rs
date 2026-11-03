use std::error::Error;
use std::fmt;

use super::{
    AssessmentLimits, AvailabilityState, ClaimAssessmentState, ClaimCheckError, ClaimInput,
    CorpusAssessmentInput, FaultCategory, PassageRef, ReviewStatus, SourceRecordInput,
    find_passage, find_record, verify_corpus,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewLimitKind {
    Reviews,
    EvidenceLinksPerReview,
    FaultCategoriesPerReview,
    TextBytesPerReview,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReviewLimits {
    max_reviews: usize,
    max_evidence_links_per_review: usize,
    max_fault_categories_per_review: usize,
    max_text_bytes_per_review: usize,
}

impl ReviewLimits {
    pub fn new(
        max_reviews: usize,
        max_evidence_links_per_review: usize,
        max_fault_categories_per_review: usize,
        max_text_bytes_per_review: usize,
    ) -> Result<Self, ReviewLedgerError> {
        if max_reviews == 0 {
            return Err(ReviewLedgerError::ZeroLimit(ReviewLimitKind::Reviews));
        }
        if max_evidence_links_per_review == 0 {
            return Err(ReviewLedgerError::ZeroLimit(
                ReviewLimitKind::EvidenceLinksPerReview,
            ));
        }
        if max_fault_categories_per_review == 0 {
            return Err(ReviewLedgerError::ZeroLimit(
                ReviewLimitKind::FaultCategoriesPerReview,
            ));
        }
        if max_text_bytes_per_review == 0 {
            return Err(ReviewLedgerError::ZeroLimit(
                ReviewLimitKind::TextBytesPerReview,
            ));
        }
        Ok(Self {
            max_reviews,
            max_evidence_links_per_review,
            max_fault_categories_per_review,
            max_text_bytes_per_review,
        })
    }

    pub fn max_reviews(self) -> usize {
        self.max_reviews
    }

    pub fn max_evidence_links_per_review(self) -> usize {
        self.max_evidence_links_per_review
    }

    pub fn max_fault_categories_per_review(self) -> usize {
        self.max_fault_categories_per_review
    }

    pub fn max_text_bytes_per_review(self) -> usize {
        self.max_text_bytes_per_review
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewTextField {
    ReviewVersionId,
    ReviewerId,
    Scope,
    RationaleAndCriteria,
}

/// A caller-authored review. The ledger borrows all identifiers, text, and evidence slices.
///
/// `reviewed_at_unix_seconds` is an explicit caller value, not a wall-clock read. Equal
/// timestamps are ordered by append order. `reviewer_id` is attribution only; this module
/// performs no authentication or authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewEntry<'a> {
    pub review_version_id: &'a str,
    pub target_claim_version_id: &'a str,
    pub supersedes_review_version_id: Option<&'a str>,
    pub reviewer_id: &'a str,
    pub reviewed_at_unix_seconds: i64,
    pub scope: &'a str,
    pub assessment: ClaimAssessmentState,
    pub review_status: ReviewStatus,
    pub rationale_and_criteria: &'a str,
    pub supporting_evidence: &'a [PassageRef<'a>],
    pub contrary_evidence: &'a [PassageRef<'a>],
    pub fault_categories: &'a [FaultCategory],
    pub needs_reassessment: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClaimReviewProjection<'a> {
    pub claim_id: &'a str,
    pub claim_version_id: &'a str,
    pub assessment_at_publication: ClaimAssessmentState,
    pub current_assessment: ClaimAssessmentState,
    pub review_status: ReviewStatus,
    pub availability: AvailabilityState,
    pub needs_reassessment: bool,
    pub latest_review_version_id: Option<&'a str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReviewLedgerError {
    InvalidBasis(ClaimCheckError),
    TargetClaimVersionNotFound {
        target_claim_version_id: String,
    },
    AmbiguousTargetClaimVersion {
        target_claim_version_id: String,
    },
    ZeroLimit(ReviewLimitKind),
    BudgetExceeded {
        kind: ReviewLimitKind,
        actual: usize,
        limit: usize,
    },
    SizeOverflow {
        kind: ReviewLimitKind,
    },
    StaleTargetClaimVersion {
        expected: String,
        actual: String,
    },
    DuplicateReviewVersionId {
        review_version_id: String,
    },
    InvalidSupersession {
        review_version_id: String,
        superseded_review_version_id: String,
        reason: &'static str,
    },
    TimestampRegression {
        review_version_id: String,
        previous_unix_seconds: i64,
        actual_unix_seconds: i64,
    },
    EmptyText {
        review_version_id: String,
        field: ReviewTextField,
    },
    UnknownPassageEvidence {
        review_version_id: String,
        source_record_version_id: String,
        passage_id: String,
        reason: &'static str,
    },
    SupportedWithoutEvidence {
        review_version_id: String,
    },
    RefutationWithoutEvidence {
        review_version_id: String,
        assessment: ClaimAssessmentState,
    },
    MateriallyMisleadingWithoutFaultCategory {
        review_version_id: String,
    },
    AllocationFailed,
}

impl fmt::Display for ReviewLedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBasis(err) => write!(f, "invalid claim review basis: {err}"),
            Self::TargetClaimVersionNotFound {
                target_claim_version_id,
            } => write!(
                f,
                "target claim version not found: {target_claim_version_id}"
            ),
            Self::AmbiguousTargetClaimVersion {
                target_claim_version_id,
            } => write!(
                f,
                "target claim version is not unique: {target_claim_version_id}"
            ),
            Self::ZeroLimit(kind) => write!(f, "{kind:?} limit must be greater than zero"),
            Self::BudgetExceeded {
                kind,
                actual,
                limit,
            } => write!(f, "{kind:?} count/size {actual} exceeds limit {limit}"),
            Self::SizeOverflow { kind } => {
                write!(f, "{kind:?} count/size overflows addressable memory")
            }
            Self::StaleTargetClaimVersion { expected, actual } => write!(
                f,
                "review targets claim version {actual}, but ledger is pinned to {expected}"
            ),
            Self::DuplicateReviewVersionId { review_version_id } => {
                write!(
                    f,
                    "duplicate review version identifier: {review_version_id}"
                )
            }
            Self::InvalidSupersession {
                review_version_id,
                superseded_review_version_id,
                reason,
            } => write!(
                f,
                "review {review_version_id} cannot supersede {superseded_review_version_id}: {reason}"
            ),
            Self::TimestampRegression {
                review_version_id,
                previous_unix_seconds,
                actual_unix_seconds,
            } => write!(
                f,
                "review {review_version_id} timestamp {actual_unix_seconds} precedes previous timestamp {previous_unix_seconds}"
            ),
            Self::EmptyText {
                review_version_id,
                field,
            } => write!(
                f,
                "review {review_version_id} has empty required text field {field:?}"
            ),

            Self::UnknownPassageEvidence {
                review_version_id,
                source_record_version_id,
                passage_id,
                reason,
            } => write!(
                f,
                "review {review_version_id} has invalid evidence {source_record_version_id}:{passage_id}: {reason}"
            ),
            Self::SupportedWithoutEvidence { review_version_id } => write!(
                f,
                "supported review {review_version_id} must cite at least one supporting passage"
            ),
            Self::RefutationWithoutEvidence {
                review_version_id,
                assessment,
            } => write!(
                f,
                "{assessment:?} review {review_version_id} must cite at least one contrary passage and state its reasoning"
            ),
            Self::MateriallyMisleadingWithoutFaultCategory { review_version_id } => write!(
                f,
                "materially misleading review {review_version_id} must declare at least one fault category"
            ),
            Self::AllocationFailed => write!(f, "memory allocation failed"),
        }
    }
}

impl Error for ReviewLedgerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidBasis(err) => Some(err),
            _ => None,
        }
    }
}

/// Bounded in-memory review history for one exact, validated claim version.
///
/// Construction runs the existing corpus verifier and borrows the target claim and immutable
/// source records. Entries are append-only; no method mutates the claim, sources, or old entries.
#[derive(Debug)]
pub struct ClaimReviewLedger<'a> {
    claim: &'a ClaimInput<'a>,
    records: &'a [SourceRecordInput<'a>],
    entries: Vec<ReviewEntry<'a>>,
    limits: ReviewLimits,
}

impl<'a> ClaimReviewLedger<'a> {
    pub fn new(
        basis: &'a CorpusAssessmentInput<'a>,
        target_claim_version_id: &'a str,
        assessment_limits: AssessmentLimits,
        limits: ReviewLimits,
    ) -> Result<Self, ReviewLedgerError> {
        let mut matching_claims = basis
            .claims
            .iter()
            .filter(|claim| claim.version_id == target_claim_version_id);
        let claim = matching_claims.next().ok_or_else(|| {
            ReviewLedgerError::TargetClaimVersionNotFound {
                target_claim_version_id: target_claim_version_id.to_owned(),
            }
        })?;
        if matching_claims.next().is_some() {
            return Err(ReviewLedgerError::AmbiguousTargetClaimVersion {
                target_claim_version_id: target_claim_version_id.to_owned(),
            });
        }

        verify_corpus(basis, assessment_limits, || Ok(()))
            .map_err(ReviewLedgerError::InvalidBasis)?;

        Ok(Self {
            claim,
            records: basis.records,
            entries: Vec::new(),
            limits,
        })
    }

    pub fn claim(&self) -> &'a ClaimInput<'a> {
        self.claim
    }

    pub fn source_records(&self) -> &'a [SourceRecordInput<'a>] {
        self.records
    }

    pub fn limits(&self) -> ReviewLimits {
        self.limits
    }

    pub fn history(&self) -> &[ReviewEntry<'a>] {
        &self.entries
    }

    pub fn latest_review(&self) -> Option<&ReviewEntry<'a>> {
        self.entries.last()
    }

    pub fn projection(&self) -> ClaimReviewProjection<'a> {
        let latest = self.entries.last();
        ClaimReviewProjection {
            claim_id: self.claim.claim_id,
            claim_version_id: self.claim.version_id,
            assessment_at_publication: self.claim.assessment_at_publication,
            current_assessment: latest
                .map_or(self.claim.current_assessment, |entry| entry.assessment),
            review_status: latest.map_or(self.claim.review_status, |entry| entry.review_status),
            availability: self.claim.availability,
            needs_reassessment: self.claim.needs_reassessment
                || self.entries.iter().any(|entry| entry.needs_reassessment),
            latest_review_version_id: latest.map(|entry| entry.review_version_id),
        }
    }

    /// Validates the complete entry before reserving or publishing it in history.
    pub fn append(&mut self, entry: ReviewEntry<'a>) -> Result<(), ReviewLedgerError> {
        if self.entries.len() >= self.limits.max_reviews {
            return Err(ReviewLedgerError::BudgetExceeded {
                kind: ReviewLimitKind::Reviews,
                actual: self.entries.len().saturating_add(1),
                limit: self.limits.max_reviews,
            });
        }

        let evidence_count = entry
            .supporting_evidence
            .len()
            .checked_add(entry.contrary_evidence.len())
            .ok_or(ReviewLedgerError::SizeOverflow {
                kind: ReviewLimitKind::EvidenceLinksPerReview,
            })?;
        if evidence_count > self.limits.max_evidence_links_per_review {
            return Err(ReviewLedgerError::BudgetExceeded {
                kind: ReviewLimitKind::EvidenceLinksPerReview,
                actual: evidence_count,
                limit: self.limits.max_evidence_links_per_review,
            });
        }
        if entry.fault_categories.len() > self.limits.max_fault_categories_per_review {
            return Err(ReviewLedgerError::BudgetExceeded {
                kind: ReviewLimitKind::FaultCategoriesPerReview,
                actual: entry.fault_categories.len(),
                limit: self.limits.max_fault_categories_per_review,
            });
        }

        let text_bytes = review_text_bytes(&entry)?;
        if text_bytes > self.limits.max_text_bytes_per_review {
            return Err(ReviewLedgerError::BudgetExceeded {
                kind: ReviewLimitKind::TextBytesPerReview,
                actual: text_bytes,
                limit: self.limits.max_text_bytes_per_review,
            });
        }

        for (text, field) in [
            (entry.review_version_id, ReviewTextField::ReviewVersionId),
            (entry.reviewer_id, ReviewTextField::ReviewerId),
            (entry.scope, ReviewTextField::Scope),
            (
                entry.rationale_and_criteria,
                ReviewTextField::RationaleAndCriteria,
            ),
        ] {
            if text.trim().is_empty() {
                return Err(ReviewLedgerError::EmptyText {
                    review_version_id: entry.review_version_id.to_owned(),
                    field,
                });
            }
        }

        if entry.target_claim_version_id != self.claim.version_id {
            return Err(ReviewLedgerError::StaleTargetClaimVersion {
                expected: self.claim.version_id.to_owned(),
                actual: entry.target_claim_version_id.to_owned(),
            });
        }
        if self
            .entries
            .iter()
            .any(|prior| prior.review_version_id == entry.review_version_id)
        {
            return Err(ReviewLedgerError::DuplicateReviewVersionId {
                review_version_id: entry.review_version_id.to_owned(),
            });
        }

        if let Some(prior) = self.entries.last()
            && entry.reviewed_at_unix_seconds < prior.reviewed_at_unix_seconds
        {
            return Err(ReviewLedgerError::TimestampRegression {
                review_version_id: entry.review_version_id.to_owned(),
                previous_unix_seconds: prior.reviewed_at_unix_seconds,
                actual_unix_seconds: entry.reviewed_at_unix_seconds,
            });
        }

        if let Some(superseded_id) = entry.supersedes_review_version_id {
            if superseded_id == entry.review_version_id {
                return Err(invalid_supersession(
                    &entry,
                    superseded_id,
                    "a review cannot supersede itself",
                ));
            }
            if !self
                .entries
                .iter()
                .any(|prior| prior.review_version_id == superseded_id)
            {
                return Err(invalid_supersession(
                    &entry,
                    superseded_id,
                    "superseded review version is not present in this ledger",
                ));
            }
            if self
                .entries
                .iter()
                .any(|prior| prior.supersedes_review_version_id == Some(superseded_id))
            {
                return Err(invalid_supersession(
                    &entry,
                    superseded_id,
                    "superseded review version has already been superseded",
                ));
            }
        }

        if entry.assessment == ClaimAssessmentState::Supported
            && entry.supporting_evidence.is_empty()
        {
            return Err(ReviewLedgerError::SupportedWithoutEvidence {
                review_version_id: entry.review_version_id.to_owned(),
            });
        }
        if matches!(
            entry.assessment,
            ClaimAssessmentState::Contradicted | ClaimAssessmentState::MateriallyMisleading
        ) && entry.contrary_evidence.is_empty()
        {
            return Err(ReviewLedgerError::RefutationWithoutEvidence {
                review_version_id: entry.review_version_id.to_owned(),
                assessment: entry.assessment,
            });
        }
        if entry.assessment == ClaimAssessmentState::MateriallyMisleading
            && entry.fault_categories.is_empty()
        {
            return Err(
                ReviewLedgerError::MateriallyMisleadingWithoutFaultCategory {
                    review_version_id: entry.review_version_id.to_owned(),
                },
            );
        }

        self.validate_evidence(&entry, entry.supporting_evidence)?;
        self.validate_evidence(&entry, entry.contrary_evidence)?;

        self.entries
            .try_reserve(1)
            .map_err(|_| ReviewLedgerError::AllocationFailed)?;
        self.entries.push(entry);
        Ok(())
    }

    fn validate_evidence(
        &self,
        entry: &ReviewEntry<'a>,
        evidence: &[PassageRef<'a>],
    ) -> Result<(), ReviewLedgerError> {
        for passage_ref in evidence {
            let Some(record) = find_record(self.records, passage_ref.source_record_version_id)
            else {
                return Err(ReviewLedgerError::UnknownPassageEvidence {
                    review_version_id: entry.review_version_id.to_owned(),
                    source_record_version_id: passage_ref.source_record_version_id.to_owned(),
                    passage_id: passage_ref.passage_id.to_owned(),
                    reason: "source record version is not in the validated review basis",
                });
            };
            if find_passage(record, passage_ref.passage_id).is_none() {
                return Err(ReviewLedgerError::UnknownPassageEvidence {
                    review_version_id: entry.review_version_id.to_owned(),
                    source_record_version_id: passage_ref.source_record_version_id.to_owned(),
                    passage_id: passage_ref.passage_id.to_owned(),
                    reason: "passage is not present in the source record version",
                });
            }
        }
        Ok(())
    }
}

fn invalid_supersession(
    entry: &ReviewEntry<'_>,
    superseded_review_version_id: &str,
    reason: &'static str,
) -> ReviewLedgerError {
    ReviewLedgerError::InvalidSupersession {
        review_version_id: entry.review_version_id.to_owned(),
        superseded_review_version_id: superseded_review_version_id.to_owned(),
        reason,
    }
}

fn review_text_bytes(entry: &ReviewEntry<'_>) -> Result<usize, ReviewLedgerError> {
    let mut total = 0usize;
    for text in [
        entry.review_version_id,
        entry.target_claim_version_id,
        entry.reviewer_id,
        entry.scope,
        entry.rationale_and_criteria,
    ] {
        add_text_bytes(&mut total, text)?;
    }
    if let Some(superseded_id) = entry.supersedes_review_version_id {
        add_text_bytes(&mut total, superseded_id)?;
    }
    for passage_ref in entry
        .supporting_evidence
        .iter()
        .chain(entry.contrary_evidence.iter())
    {
        add_text_bytes(&mut total, passage_ref.source_record_version_id)?;
        add_text_bytes(&mut total, passage_ref.passage_id)?;
    }
    Ok(total)
}

fn add_text_bytes(total: &mut usize, text: &str) -> Result<(), ReviewLedgerError> {
    *total = total.checked_add(text.len()).ok_or({
        ReviewLedgerError::SizeOverflow {
            kind: ReviewLimitKind::TextBytesPerReview,
        }
    })?;
    Ok(())
}
