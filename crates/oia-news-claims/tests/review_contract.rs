use oia_news_claims::review::{
    ClaimReviewLedger, ReviewEntry, ReviewLedgerError, ReviewLimitKind, ReviewLimits,
    ReviewTextField,
};
use oia_news_claims::{
    AssessmentLimits, AvailabilityState, ClaimAssessmentState, ClaimCheckError, ClaimInput,
    CorpusAssessmentInput, FaultCategory, PassageAnchor, PassageRef, RetractionStatus,
    ReviewStatus, SourceRecordInput,
};

const ARTICLE_QUOTE: &str = "The report claims a corridor closure.";
const EVIDENCE_QUOTE: &str = "The official log records that the corridor remained open.";
const ARTICLE_RAW: &[u8] = b"The report claims a corridor closure.";
const EVIDENCE_RAW: &[u8] = b"The official log records that the corridor remained open.";
const SHA256_FORMAT_ONLY: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const ARTICLE_PASSAGES: &[PassageAnchor<'static>] = &[PassageAnchor {
    passage_id: "p-claim",
    language: "en",
    byte_range: 0..37,
    char_range: 0..37,
    quote: ARTICLE_QUOTE,
}];
const EVIDENCE_PASSAGES: &[PassageAnchor<'static>] = &[PassageAnchor {
    passage_id: "p-official-log",
    language: "en",
    byte_range: 0..57,
    char_range: 0..57,
    quote: EVIDENCE_QUOTE,
}];
const CONTRARY_EVIDENCE: &[PassageRef<'static>] = &[PassageRef {
    source_record_version_id: "record-evidence@v1",
    passage_id: "p-official-log",
}];
const TWO_SUPPORTING_PASSAGES: &[PassageRef<'static>] = &[
    PassageRef {
        source_record_version_id: "record-evidence@v1",
        passage_id: "p-official-log",
    },
    PassageRef {
        source_record_version_id: "record-article@v1",
        passage_id: "p-claim",
    },
];
const UNKNOWN_EVIDENCE: &[PassageRef<'static>] = &[PassageRef {
    source_record_version_id: "record-evidence@v1",
    passage_id: "p-unknown",
}];
const REFUTATION_FAULTS: &[FaultCategory] = &[FaultCategory::FactualContradiction];
const LONG_SCOPE: &str = "1234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890";

fn synthetic_records() -> Vec<SourceRecordInput<'static>> {
    vec![
        SourceRecordInput {
            record_id: "record-article",
            version_id: "record-article@v1",
            source_id: "source-article",
            predecessor_version_id: None,
            superseded_by_version_id: None,
            availability: AvailabilityState::Available,
            retraction_status: RetractionStatus::NotRetracted,
            review_status: ReviewStatus::Unreviewed,
            needs_reassessment: false,
            independent_lineage_root_id: "root-article",
            is_syndicated_reprint: false,
            raw_bytes: ARTICLE_RAW,
            declared_sha256_hex: SHA256_FORMAT_ONLY,
            expected_char_length: ARTICLE_QUOTE.len(),
            passages: ARTICLE_PASSAGES,
        },
        SourceRecordInput {
            record_id: "record-evidence",
            version_id: "record-evidence@v1",
            source_id: "source-evidence",
            predecessor_version_id: None,
            superseded_by_version_id: None,
            availability: AvailabilityState::Available,
            retraction_status: RetractionStatus::NotRetracted,
            review_status: ReviewStatus::Reviewed,
            needs_reassessment: false,
            independent_lineage_root_id: "root-evidence",
            is_syndicated_reprint: false,
            raw_bytes: EVIDENCE_RAW,
            declared_sha256_hex: SHA256_FORMAT_ONLY,
            expected_char_length: EVIDENCE_QUOTE.len(),
            passages: EVIDENCE_PASSAGES,
        },
    ]
}

fn synthetic_claim() -> ClaimInput<'static> {
    ClaimInput {
        claim_id: "claim-1",
        version_id: "claim-1@v1",
        source_record_version_id: "record-article@v1",
        passage_id: "p-claim",
        headline_passage_id: None,
        proposition: "The corridor was closed.",
        claimant: "Synthetic article",
        quotes_claim_without_endorsing: false,
        is_opinion_not_factual_premise: false,
        assessment_at_publication: ClaimAssessmentState::Disputed,
        current_assessment: ClaimAssessmentState::Unresolved,
        review_status: ReviewStatus::Unreviewed,
        availability: AvailabilityState::Available,
        needs_reassessment: false,
        fault_categories: &[],
        supporting_evidence: &[],
        contrary_evidence: &[],
        criteria_and_reasoning: "Publication-time accounts disagreed and the retained evidence was incomplete.",
    }
}

fn assessment_limits() -> AssessmentLimits {
    AssessmentLimits::new(4, 4, 4, 4, 4, 4, 4, 4, 4096)
        .unwrap_or_else(|err| panic!("valid synthetic assessment limits: {err}"))
}

fn review_limits(max_reviews: usize, max_text_bytes: usize) -> ReviewLimits {
    ReviewLimits::new(max_reviews, 2, 2, max_text_bytes)
        .unwrap_or_else(|err| panic!("valid synthetic review limits: {err}"))
}

fn candidate_review() -> ReviewEntry<'static> {
    ReviewEntry {
        review_version_id: "review-candidate",
        target_claim_version_id: "claim-1@v1",
        supersedes_review_version_id: None,
        reviewer_id: "reviewer-1",
        reviewed_at_unix_seconds: 101,
        scope: "claim text and official log",
        assessment: ClaimAssessmentState::Unresolved,
        review_status: ReviewStatus::Reviewed,
        rationale_and_criteria: "Compare the exact claim passage with the retained official log.",
        supporting_evidence: &[],
        contrary_evidence: &[],
        fault_categories: &[],
        needs_reassessment: false,
    }
}

fn rejected_without_mutation<'a>(
    ledger: &mut ClaimReviewLedger<'a>,
    entry: ReviewEntry<'a>,
) -> ReviewLedgerError {
    let history_len = ledger.history().len();
    let latest = ledger.latest_review().cloned();
    let projection = ledger.projection();
    let error = match ledger.append(entry) {
        Ok(()) => panic!("invalid review unexpectedly appended"),
        Err(error) => error,
    };
    assert_eq!(ledger.history().len(), history_len);
    assert_eq!(ledger.latest_review(), latest.as_ref());
    assert_eq!(ledger.projection(), projection);
    error
}

#[test]
fn preserves_challenge_review_history_and_unresolved_reassessment() {
    let records = synthetic_records();
    let claims = [synthetic_claim()];
    let source_ids = ["source-article", "source-evidence"];
    let basis = CorpusAssessmentInput {
        source_ids: &source_ids,
        records: &records,
        lineage_edges: &[],
        candidates: &[],
        claims: &claims,
        invalidations: &[],
        publisher_summaries: &[],
    };
    let mut ledger = ClaimReviewLedger::new(
        &basis,
        "claim-1@v1",
        assessment_limits(),
        review_limits(4, 1024),
    )
    .unwrap_or_else(|err| panic!("valid review basis: {err}"));

    let challenge = ReviewEntry {
        review_version_id: "review-challenge@v1",
        target_claim_version_id: "claim-1@v1",
        supersedes_review_version_id: None,
        reviewer_id: "reviewer-challenge",
        reviewed_at_unix_seconds: 100,
        scope: "later official-log correction",
        assessment: ClaimAssessmentState::Unresolved,
        review_status: ReviewStatus::Challenged,
        rationale_and_criteria: "A later official log challenges the closure report and requires reassessment.",
        supporting_evidence: &[],
        contrary_evidence: CONTRARY_EVIDENCE,
        fault_categories: REFUTATION_FAULTS,
        needs_reassessment: true,
    };
    let challenge_snapshot = challenge.clone();
    ledger
        .append(challenge)
        .unwrap_or_else(|err| panic!("valid challenge: {err}"));
    assert_eq!(ledger.projection().review_status, ReviewStatus::Challenged);
    assert!(ledger.projection().needs_reassessment);

    let resolution = ReviewEntry {
        review_version_id: "review-resolution@v1",
        target_claim_version_id: "claim-1@v1",
        supersedes_review_version_id: Some("review-challenge@v1"),
        reviewer_id: "reviewer-resolution",
        reviewed_at_unix_seconds: 200,
        scope: "exact passage and official-log refutation",
        assessment: ClaimAssessmentState::Contradicted,
        review_status: ReviewStatus::Reviewed,
        rationale_and_criteria: "The independent official log explicitly says the corridor remained open.",
        supporting_evidence: &[],
        contrary_evidence: CONTRARY_EVIDENCE,
        fault_categories: REFUTATION_FAULTS,
        needs_reassessment: false,
    };
    ledger
        .append(resolution)
        .unwrap_or_else(|err| panic!("valid resolving review: {err}"));

    assert_eq!(ledger.history().len(), 2);
    assert_eq!(ledger.history().first(), Some(&challenge_snapshot));
    assert_eq!(
        ledger.latest_review().map(|entry| entry.review_version_id),
        Some("review-resolution@v1")
    );
    let projection = ledger.projection();
    assert_eq!(projection.claim_version_id, "claim-1@v1");
    assert_eq!(
        projection.assessment_at_publication,
        ClaimAssessmentState::Disputed
    );
    assert_eq!(
        projection.current_assessment,
        ClaimAssessmentState::Contradicted
    );
    assert_eq!(projection.review_status, ReviewStatus::Reviewed);
    assert_ne!(
        projection.current_assessment,
        ClaimAssessmentState::Supported
    );
    assert_eq!(projection.availability, AvailabilityState::Available);
    assert!(projection.needs_reassessment);
    assert_eq!(
        projection.latest_review_version_id,
        Some("review-resolution@v1")
    );

    assert_eq!(
        ledger.claim().assessment_at_publication,
        ClaimAssessmentState::Disputed
    );
    assert_eq!(
        ledger.claim().current_assessment,
        ClaimAssessmentState::Unresolved
    );
    assert!(!ledger.claim().needs_reassessment);
    assert!(
        ledger
            .source_records()
            .iter()
            .all(|record| record.availability == AvailabilityState::Available)
    );
}

#[test]
fn rejects_invalid_reviews_without_mutating_history_or_projection() {
    let claims = [synthetic_claim()];
    let source_ids = ["source-article", "source-evidence"];

    let invalid_passages = [PassageAnchor {
        passage_id: "p-claim",
        language: "en",
        byte_range: 0..36,
        char_range: 0..36,
        quote: ARTICLE_QUOTE,
    }];
    let mut invalid_records = synthetic_records();
    if let Some(article) = invalid_records.first_mut() {
        article.passages = &invalid_passages;
    }
    let invalid_basis = CorpusAssessmentInput {
        source_ids: &source_ids,
        records: &invalid_records,
        lineage_edges: &[],
        candidates: &[],
        claims: &claims,
        invalidations: &[],
        publisher_summaries: &[],
    };
    let invalid_ledger = ClaimReviewLedger::new(
        &invalid_basis,
        "claim-1@v1",
        assessment_limits(),
        review_limits(2, 256),
    );
    assert!(matches!(
        invalid_ledger,
        Err(ReviewLedgerError::InvalidBasis(
            ClaimCheckError::InvalidPassageAnchor { .. }
        ))
    ));

    let records = synthetic_records();
    let basis = CorpusAssessmentInput {
        source_ids: &source_ids,
        records: &records,
        lineage_edges: &[],
        candidates: &[],
        claims: &claims,
        invalidations: &[],
        publisher_summaries: &[],
    };
    let mut ledger = ClaimReviewLedger::new(
        &basis,
        "claim-1@v1",
        assessment_limits(),
        review_limits(2, 256),
    )
    .unwrap_or_else(|err| panic!("valid review basis: {err}"));
    let baseline = ReviewEntry {
        review_version_id: "review-base",
        reviewed_at_unix_seconds: 100,
        ..candidate_review()
    };
    ledger
        .append(baseline)
        .unwrap_or_else(|err| panic!("valid baseline review: {err}"));

    let stale = ReviewEntry {
        target_claim_version_id: "claim-1@v2",
        ..candidate_review()
    };
    assert!(matches!(
        rejected_without_mutation(&mut ledger, stale),
        ReviewLedgerError::StaleTargetClaimVersion { .. }
    ));

    let duplicate = ReviewEntry {
        review_version_id: "review-base",
        ..candidate_review()
    };
    assert!(matches!(
        rejected_without_mutation(&mut ledger, duplicate),
        ReviewLedgerError::DuplicateReviewVersionId { .. }
    ));

    let unknown_supersession = ReviewEntry {
        supersedes_review_version_id: Some("review-missing"),
        ..candidate_review()
    };
    assert!(matches!(
        rejected_without_mutation(&mut ledger, unknown_supersession),
        ReviewLedgerError::InvalidSupersession { .. }
    ));

    let self_supersession = ReviewEntry {
        supersedes_review_version_id: Some("review-candidate"),
        ..candidate_review()
    };
    assert!(matches!(
        rejected_without_mutation(&mut ledger, self_supersession),
        ReviewLedgerError::InvalidSupersession { .. }
    ));

    let regressed = ReviewEntry {
        reviewed_at_unix_seconds: 99,
        ..candidate_review()
    };
    assert!(matches!(
        rejected_without_mutation(&mut ledger, regressed),
        ReviewLedgerError::TimestampRegression { .. }
    ));

    for (entry, expected_field) in [
        (
            ReviewEntry {
                reviewer_id: "  ",
                ..candidate_review()
            },
            ReviewTextField::ReviewerId,
        ),
        (
            ReviewEntry {
                scope: "",
                ..candidate_review()
            },
            ReviewTextField::Scope,
        ),
        (
            ReviewEntry {
                rationale_and_criteria: "\n\t",
                ..candidate_review()
            },
            ReviewTextField::RationaleAndCriteria,
        ),
    ] {
        let error = rejected_without_mutation(&mut ledger, entry);
        assert!(matches!(
            error,
            ReviewLedgerError::EmptyText { field, .. } if field == expected_field
        ));
    }

    let unknown_evidence = ReviewEntry {
        contrary_evidence: UNKNOWN_EVIDENCE,
        ..candidate_review()
    };
    assert!(matches!(
        rejected_without_mutation(&mut ledger, unknown_evidence),
        ReviewLedgerError::UnknownPassageEvidence { .. }
    ));

    let supported_without_evidence = ReviewEntry {
        assessment: ClaimAssessmentState::Supported,
        ..candidate_review()
    };
    assert!(matches!(
        rejected_without_mutation(&mut ledger, supported_without_evidence),
        ReviewLedgerError::SupportedWithoutEvidence { .. }
    ));

    let contradicted_without_evidence = ReviewEntry {
        assessment: ClaimAssessmentState::Contradicted,
        ..candidate_review()
    };
    assert!(matches!(
        rejected_without_mutation(&mut ledger, contradicted_without_evidence),
        ReviewLedgerError::RefutationWithoutEvidence {
            assessment: ClaimAssessmentState::Contradicted,
            ..
        }
    ));

    let misleading_without_evidence = ReviewEntry {
        assessment: ClaimAssessmentState::MateriallyMisleading,
        fault_categories: REFUTATION_FAULTS,
        ..candidate_review()
    };
    assert!(matches!(
        rejected_without_mutation(&mut ledger, misleading_without_evidence),
        ReviewLedgerError::RefutationWithoutEvidence {
            assessment: ClaimAssessmentState::MateriallyMisleading,
            ..
        }
    ));

    let excessive_evidence = ReviewEntry {
        assessment: ClaimAssessmentState::Supported,
        supporting_evidence: TWO_SUPPORTING_PASSAGES,
        contrary_evidence: CONTRARY_EVIDENCE,
        ..candidate_review()
    };
    assert!(matches!(
        rejected_without_mutation(&mut ledger, excessive_evidence),
        ReviewLedgerError::BudgetExceeded {
            kind: ReviewLimitKind::EvidenceLinksPerReview,
            actual: 3,
            limit: 2,
        }
    ));

    let excessive_text = ReviewEntry {
        scope: LONG_SCOPE,
        ..candidate_review()
    };
    assert!(matches!(
        rejected_without_mutation(&mut ledger, excessive_text),
        ReviewLedgerError::BudgetExceeded {
            kind: ReviewLimitKind::TextBytesPerReview,
            ..
        }
    ));

    let second = ReviewEntry {
        review_version_id: "review-second",
        reviewed_at_unix_seconds: 100,
        ..candidate_review()
    };
    ledger
        .append(second)
        .unwrap_or_else(|err| panic!("valid equal-timestamp review: {err}"));
    assert_eq!(
        ledger.latest_review().map(|entry| entry.review_version_id),
        Some("review-second")
    );

    let excessive_history = ReviewEntry {
        review_version_id: "review-third",
        ..candidate_review()
    };
    assert!(matches!(
        rejected_without_mutation(&mut ledger, excessive_history),
        ReviewLedgerError::BudgetExceeded {
            kind: ReviewLimitKind::Reviews,
            actual: 3,
            limit: 2,
        }
    ));
}
