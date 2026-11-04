//! Targeted `NEWS-02` regional-account contracts: syndicated copies do not add
//! independent sources (even when they declare their own lineage roots), legal
//! stages keep their order without erasing earlier stages, observed movement
//! cannot carry a purpose, window coverage keeps uncertainty-only inclusions and
//! exclusions visible, and member lists are bounded before any lookup.

use oia_news_claims::regional::{
    DevelopmentKind, EpistemicCategory, LocationBasis, ProposedEventInput, PurposeKind,
    RegionalAccount, RegionalAccountError, RegionalAccountInput, RegionalCandidateInput,
    RegionalLimitKind, RegionalLimits, ReportedInterval, ReportedLocation, StatedPurposeInput,
    WindowOverlap, build_regional_account,
};
use oia_news_claims::{
    AssessmentLimits, AvailabilityState, CorpusAssessmentInput, EventCandidateInput,
    LineageEdgeInput, LineageRelationKind, PassageAnchor, RetractionStatus, ReviewStatus,
    SourceRecordInput,
};

const SHA256_FORMAT_ONLY: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const MOVE: &str = "Two patrol boats moved toward the north pier at dawn.";
const STATEMENT: &str = "The harbor master said the boats would escort the morning ferry.";
const COURT: &str =
    "The skipper was detained, charged with reckless operation, and later acquitted.";
const ADRIFT: &str = "A tug was reported adrift near the south breakwater.";
const REGION: &str = "region-synthetic-harbor";

const fn whole(passage_id: &'static str, text: &'static str) -> PassageAnchor<'static> {
    PassageAnchor {
        passage_id,
        language: "en",
        byte_range: 0..text.len(),
        char_range: 0..text.len(),
        quote: text,
    }
}

const MOVE_P: &[PassageAnchor<'static>] = &[whole("p-move", MOVE)];
const STATEMENT_P: &[PassageAnchor<'static>] = &[whole("p-statement", STATEMENT)];
const REPRINT_P: &[PassageAnchor<'static>] = &[whole("p-reprint", STATEMENT)];
const COURT_P: &[PassageAnchor<'static>] = &[whole("p-court", COURT)];
const ADRIFT_P: &[PassageAnchor<'static>] = &[whole("p-adrift", ADRIFT)];

fn record(
    version_id: &'static str,
    source_id: &'static str,
    root: &'static str,
    reprint: bool,
    text: &'static str,
    passages: &'static [PassageAnchor<'static>],
) -> SourceRecordInput<'static> {
    SourceRecordInput {
        record_id: version_id.trim_end_matches("@v1"),
        version_id,
        source_id,
        predecessor_version_id: None,
        superseded_by_version_id: None,
        availability: AvailabilityState::Available,
        retraction_status: RetractionStatus::NotRetracted,
        review_status: ReviewStatus::Reviewed,
        needs_reassessment: false,
        independent_lineage_root_id: root,
        is_syndicated_reprint: reprint,
        raw_bytes: text.as_bytes(),
        declared_sha256_hex: SHA256_FORMAT_ONLY,
        expected_char_length: text.len(),
        passages,
    }
}

const SOURCES: &[&str] = &[
    "src-port-log",
    "src-harbor-office",
    "src-aggregator",
    "src-court",
    "src-wire-a",
    "src-wire-b",
];

fn records() -> Vec<SourceRecordInput<'static>> {
    vec![
        record(
            "rec-patrol@v1",
            "src-port-log",
            "root-port",
            false,
            MOVE,
            MOVE_P,
        ),
        record(
            "rec-statement@v1",
            "src-harbor-office",
            "root-office",
            false,
            STATEMENT,
            STATEMENT_P,
        ),
        record(
            "rec-reprint@v1",
            "src-aggregator",
            "root-office",
            true,
            STATEMENT,
            REPRINT_P,
        ),
        record(
            "rec-court@v1",
            "src-court",
            "root-court",
            false,
            COURT,
            COURT_P,
        ),
        record(
            "rec-adrift@v1",
            "src-port-log",
            "root-adrift",
            false,
            ADRIFT,
            ADRIFT_P,
        ),
    ]
}

fn base(id: &'static str, rec: &'static str, review: ReviewStatus) -> EventCandidateInput<'static> {
    EventCandidateInput {
        candidate_id: id,
        source_record_version_id: rec,
        review_status: review,
        needs_reassessment: review != ReviewStatus::Reviewed,
        movement_establishes_intent_or_mission: false,
    }
}

fn corpus_candidates() -> Vec<EventCandidateInput<'static>> {
    vec![
        base("cand-move", "rec-patrol@v1", ReviewStatus::Reviewed),
        base("cand-statement", "rec-statement@v1", ReviewStatus::Reviewed),
        base("cand-arrest", "rec-court@v1", ReviewStatus::Reviewed),
        base("cand-charge", "rec-court@v1", ReviewStatus::Superseded),
        base("cand-judgment", "rec-court@v1", ReviewStatus::Reviewed),
        base("cand-adrift-near", "rec-adrift@v1", ReviewStatus::Reviewed),
        base("cand-adrift-far", "rec-adrift@v1", ReviewStatus::Reviewed),
    ]
}

const POINT: ReportedLocation = ReportedLocation {
    basis: LocationBasis::PointWithUncertaintyRadius,
    uncertainty_m: Some(100.0),
};

fn at(
    start: &'static str,
    end: &'static str,
    uncertainty_s: u32,
) -> Option<ReportedInterval<'static>> {
    Some(ReportedInterval {
        start_utc: start,
        end_utc: end,
        uncertainty_s,
    })
}

fn candidate(
    id: &'static str,
    event: &'static str,
    category: EpistemicCategory,
    passage: &'static [&'static str],
    time: Option<ReportedInterval<'static>>,
) -> RegionalCandidateInput<'static> {
    RegionalCandidateInput {
        candidate_id: id,
        proposed_event_id: event,
        category,
        development_kind: DevelopmentKind::PublicSafetyIncident,
        attributed_speaker: None,
        supporting_passage_ids: passage,
        reported_time: time,
        location: POINT,
        reports_movement: false,
        stated_purpose: None,
        legal_matter_id: None,
    }
}

fn regional_candidates() -> Vec<RegionalCandidateInput<'static>> {
    let court = |id, category| RegionalCandidateInput {
        legal_matter_id: Some("matter-skipper"),
        ..candidate(
            id,
            "evt-court",
            category,
            &["p-court"],
            at("2024-05-01T06:00:00Z", "2024-05-01T06:00:00Z", 0),
        )
    };
    vec![
        RegionalCandidateInput {
            development_kind: DevelopmentKind::MilitaryMovement,
            reports_movement: true,
            ..candidate(
                "cand-move",
                "evt-pier",
                EpistemicCategory::Observation,
                &["p-move"],
                at("2024-05-01T05:00:00Z", "2024-05-01T05:30:00Z", 60),
            )
        },
        RegionalCandidateInput {
            attributed_speaker: Some("The harbor master"),
            ..candidate(
                "cand-statement",
                "evt-pier",
                EpistemicCategory::AttributedStatement,
                &["p-statement"],
                None,
            )
        },
        // Deliberately out of stage order.
        court("cand-judgment", EpistemicCategory::Judgment),
        court("cand-charge", EpistemicCategory::Charge),
        court("cand-arrest", EpistemicCategory::Arrest),
        candidate(
            "cand-adrift-near",
            "evt-adrift-near",
            EpistemicCategory::Observation,
            &["p-adrift"],
            at("2024-04-30T23:00:00Z", "2024-04-30T23:50:00Z", 1_800),
        ),
        candidate(
            "cand-adrift-far",
            "evt-adrift-far",
            EpistemicCategory::Observation,
            &["p-adrift"],
            at("2024-04-01T00:00:00Z", "2024-04-01T01:00:00Z", 0),
        ),
    ]
}

const fn event(id: &'static str, extra: &'static [&'static str]) -> ProposedEventInput<'static> {
    ProposedEventInput {
        event_id: id,
        version_id: id,
        region_id: REGION,
        grouping_decision_id: "decision-1",
        additional_member_version_ids: extra,
        conflicts: &[],
    }
}

const EVENTS: &[ProposedEventInput<'static>] = &[
    event("evt-pier", &["rec-reprint@v1"]),
    event("evt-court", &[]),
    event("evt-adrift-near", &[]),
    event("evt-adrift-far", &[]),
];

/// Builds the account and hands it to `inspect`; the account borrows the local corpus.
fn build<R>(
    candidates: &[RegionalCandidateInput<'_>],
    inspect: impl FnOnce(Result<RegionalAccount<'_>, RegionalAccountError>) -> R,
) -> R {
    build_in(&records(), &[], EVENTS, candidates, inspect)
}

fn build_in<R>(
    records: &[SourceRecordInput<'_>],
    lineage_edges: &[LineageEdgeInput<'_>],
    events: &[ProposedEventInput<'_>],
    candidates: &[RegionalCandidateInput<'_>],
    inspect: impl FnOnce(Result<RegionalAccount<'_>, RegionalAccountError>) -> R,
) -> R {
    let corpus_candidates = corpus_candidates();
    let corpus = CorpusAssessmentInput {
        source_ids: SOURCES,
        records,
        lineage_edges,
        candidates: &corpus_candidates,
        claims: &[],
        invalidations: &[],
        publisher_summaries: &[],
    };
    let input = RegionalAccountInput {
        region_id: REGION,
        window_start_utc: "2024-05-01T00:00:00Z",
        window_end_utc: "2024-06-30T23:59:59Z",
        events,
        candidates,
    };
    let result = AssessmentLimits::new(8, 8, 4, 4, 4, 4, 4, 4, 1_024)
        .map_err(RegionalAccountError::Corpus)
        .and_then(|assessment| {
            let limits = RegionalLimits::new(8, 16, 8, 4, 4, 256)?;
            build_regional_account(&corpus, assessment, &input, limits, || Ok(()))
        });
    inspect(result)
}

fn build_err(candidates: &[RegionalCandidateInput<'_>]) -> Option<RegionalAccountError> {
    build(candidates, |result| result.err())
}

fn expect_account<R>(
    candidates: &[RegionalCandidateInput<'_>],
    inspect: impl FnOnce(&RegionalAccount<'_>) -> R,
) -> R {
    build(candidates, |result| match result {
        Ok(account) => inspect(&account),
        Err(err) => panic!("account should build: {err}"),
    })
}

#[test]
fn counts_stay_separate_and_legal_stages_keep_order_without_erasing_earlier_stages() {
    expect_account(&regional_candidates(), |account| {
        let Some(pier) = account.events.iter().find(|e| e.event_id == "evt-pier") else {
            panic!("evt-pier missing");
        };
        // The syndicated reprint adds an article and a version but no independent source.
        assert_eq!(pier.counts.articles, 3);
        assert_eq!(pier.counts.article_versions, 3);
        assert_eq!(pier.counts.syndicated_reprint_versions, 1);
        assert_eq!(pier.counts.independent_sources, 2);
        assert_eq!(pier.counts.entries, 2);

        let Some(court) = account.events.iter().find(|e| e.event_id == "evt-court") else {
            panic!("evt-court missing");
        };
        let [matter] = court.legal_matters.as_slice() else {
            panic!("expected one legal matter");
        };
        let stages: Vec<_> = matter.stages.iter().map(|s| s.category).collect();
        assert_eq!(
            stages,
            [
                EpistemicCategory::Arrest,
                EpistemicCategory::Charge,
                EpistemicCategory::Judgment
            ]
        );
        assert_eq!(matter.latest_reported_stage, EpistemicCategory::Judgment);
        assert!(matter.judgment_reported);
        assert!(
            matter
                .stages
                .iter()
                .any(|s| s.category == EpistemicCategory::Charge
                    && s.review_status == ReviewStatus::Superseded),
            "the superseded charge stays visible"
        );
    });
}

#[test]
fn observed_movement_cannot_carry_a_purpose_but_an_attributed_statement_can() {
    let purpose = |passage_id| StatedPurposeInput {
        kind: PurposeKind::Mission,
        text: "escort the morning ferry",
        passage_id,
    };

    let mut candidates = regional_candidates();
    if let Some(observation) = candidates
        .iter_mut()
        .find(|c| c.candidate_id == "cand-move")
    {
        observation.stated_purpose = Some(purpose("p-move"));
    }
    assert_eq!(
        build_err(&candidates),
        Some(RegionalAccountError::MovementPromotedToPurpose {
            candidate_id: "cand-move".to_owned()
        })
    );

    let mut candidates = regional_candidates();
    if let Some(statement) = candidates
        .iter_mut()
        .find(|c| c.candidate_id == "cand-statement")
    {
        statement.stated_purpose = Some(purpose("p-statement"));
    }
    expect_account(&candidates, |account| {
        let purpose = account
            .events
            .iter()
            .flat_map(|e| &e.entries)
            .find(|e| e.candidate_id == "cand-statement")
            .and_then(|e| e.purpose);
        assert_eq!(purpose.map(|p| p.attributed_to), Some("The harbor master"));
        assert_eq!(
            purpose.map(|p| p.category),
            Some(EpistemicCategory::AttributedStatement)
        );
    });

    if let Some(statement) = candidates
        .iter_mut()
        .find(|c| c.candidate_id == "cand-statement")
    {
        statement.attributed_speaker = None;
    }
    assert_eq!(
        build_err(&candidates),
        Some(RegionalAccountError::MissingAttribution {
            candidate_id: "cand-statement".to_owned()
        })
    );
}

#[test]
fn window_coverage_keeps_uncertainty_only_unreported_and_excluded_items_visible() {
    expect_account(&regional_candidates(), |account| {
        let overlap = |id: &str| {
            account
                .events
                .iter()
                .flat_map(|e| &e.entries)
                .find(|e| e.candidate_id == id)
                .map(|e| e.window_overlap)
        };
        assert_eq!(
            overlap("cand-adrift-near"),
            Some(WindowOverlap::UncertaintyOnly)
        );
        assert_eq!(
            overlap("cand-statement"),
            Some(WindowOverlap::TimeNotReported)
        );
        assert_eq!(overlap("cand-adrift-far"), None);
        assert_eq!(account.coverage.events_considered, 4);
        assert_eq!(account.coverage.events_included, 3);
        assert_eq!(account.coverage.events_outside_window, 1);
        assert_eq!(account.coverage.entries_uncertainty_only, 1);
        assert_eq!(account.coverage.entries_time_not_reported, 1);
    });
}

const WIRE_A: &str = "Wire copy: the harbor master said boats would escort the ferry.";
const WIRE_B: &str = "Digest: boats to escort the morning ferry, the harbor master said.";
const WIRE_A_P: &[PassageAnchor<'static>] = &[whole("p-wire-a", WIRE_A)];
const WIRE_B_P: &[PassageAnchor<'static>] = &[whole("p-wire-b", WIRE_B)];

#[test]
fn independence_is_derived_from_lineage_not_from_declared_roots() {
    let mut corpus_records = records();
    // Reprints that declare their own roots: one is linked to its original by a
    // `Republishes` edge, the other has no identified original in the corpus.
    corpus_records.push(record(
        "rec-wire-a@v1",
        "src-wire-a",
        "root-wire-a",
        true,
        WIRE_A,
        WIRE_A_P,
    ));
    corpus_records.push(record(
        "rec-wire-b@v1",
        "src-wire-b",
        "root-wire-b",
        true,
        WIRE_B,
        WIRE_B_P,
    ));
    let edges = [LineageEdgeInput {
        edge_id: "edge-wire-a",
        relation_kind: LineageRelationKind::Republishes,
        from_version_id: "rec-wire-a@v1",
        to_version_id: "rec-statement@v1",
        preserves_independent_corroboration: false,
    }];
    let events = [
        event(
            "evt-pier",
            &["rec-reprint@v1", "rec-wire-a@v1", "rec-wire-b@v1"],
        ),
        event("evt-court", &[]),
        event("evt-adrift-near", &[]),
        event("evt-adrift-far", &[]),
    ];
    build_in(
        &corpus_records,
        &edges,
        &events,
        &regional_candidates(),
        |result| {
            let account = result.unwrap_or_else(|err| panic!("account should build: {err}"));
            let Some(pier) = account.events.iter().find(|e| e.event_id == "evt-pier") else {
                panic!("evt-pier missing");
            };
            assert_eq!(pier.counts.declared_lineage_roots, 4);
            assert_eq!(pier.counts.independent_sources, 2);
            assert_eq!(pier.counts.unanchored_reprint_classes, 1);
            assert_eq!(account.unanchored_reprint_classes, 1);
        },
    );

    // An empty declared root on a member record is refused, not counted.
    let mut empty_root = records();
    if let Some(reprint) = empty_root
        .iter_mut()
        .find(|r| r.version_id == "rec-reprint@v1")
    {
        reprint.independent_lineage_root_id = "";
    }
    build_in(&empty_root, &[], EVENTS, &regional_candidates(), |result| {
        assert!(matches!(
            result,
            Err(RegionalAccountError::EmptyField {
                field: "independent_lineage_root_id",
                ..
            })
        ));
    });
}

#[test]
fn member_lists_are_bounded_and_unique_before_lookup() {
    // Ids that are not in the corpus prove the refusal happens before lookup.
    let duplicate = [event("evt-pier", &["rec-missing@v1", "rec-missing@v1"])];
    build_in(&records(), &[], &duplicate, &[], |result| {
        assert_eq!(
            result.err(),
            Some(RegionalAccountError::DuplicateIdentifier {
                kind: "additional_member_version_id",
                id: "rec-missing@v1".to_owned(),
            })
        );
    });
    let over_limit = [event(
        "evt-pier",
        &["m1", "m2", "m3", "m4", "m5", "m6", "m7", "m8", "m9"],
    )];
    build_in(&records(), &[], &over_limit, &[], |result| {
        assert_eq!(
            result.err(),
            Some(RegionalAccountError::BudgetExceeded {
                kind: RegionalLimitKind::MembersPerEvent,
                actual: 9,
                limit: 8,
            })
        );
    });
}
