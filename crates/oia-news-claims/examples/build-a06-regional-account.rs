//! Builds the `NEWS-02` regional account for the synthetic `A06` corpus and
//! checks it against the counts recorded in `fixtures/news/a06/manifest.json`
//! (`event_clusters.scenario_counts` and `active_clusters[*].counts`).
//!
//! Region id, development kinds, legal matter id, and stated purposes are
//! analyst-declared typing for this fixture; the fixture itself does not carry
//! them. Grouping follows the fixture's `cluster-rev-2` split decision.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use oia_news_claims::regional::{
    ConflictResolution, DevelopmentKind, EpistemicCategory, EventConflictInput, LocationBasis,
    ProposedEventInput, PurposeKind, RegionalAccount, RegionalAccountInput, RegionalCandidateInput,
    RegionalLimits, ReportedInterval, ReportedLocation, StatedPurposeInput, build_regional_account,
};
use oia_news_claims::{
    AssessmentLimits, AvailabilityState, CorpusAssessmentInput, EventCandidateInput,
    LineageEdgeInput, LineageRelationKind, PassageAnchor, RetractionStatus, ReviewStatus,
    SourceRecordInput, StopReason,
};

const PELAGIC_NOTICE_V1: &[u8] =
    include_bytes!("../../../fixtures/news/a06/blobs/pelagic_notice_ca_0041_v1.txt");
const HARBOR_LOG_V1: &[u8] =
    include_bytes!("../../../fixtures/news/a06/blobs/harbor_coop_log_v1.txt");
const WIRE_V1: &[u8] =
    include_bytes!("../../../fixtures/news/a06/blobs/pacific_wire_dispatch_v1.txt");
const WIRE_V2: &[u8] =
    include_bytes!("../../../fixtures/news/a06/blobs/pacific_wire_dispatch_v2.txt");
const TRIBUNE_V1: &[u8] =
    include_bytes!("../../../fixtures/news/a06/blobs/island_tribune_syndicated_v1.txt");
const MARITIME_DAILY_V1: &[u8] =
    include_bytes!("../../../fixtures/news/a06/blobs/maritime_daily_syndicated_v1.txt");
const PORT_SAFETY_V1: &[u8] =
    include_bytes!("../../../fixtures/news/a06/blobs/port_authority_bulletin_v1.txt");
const DOCKET_V1: &[u8] =
    include_bytes!("../../../fixtures/news/a06/blobs/district_court_docket_v1.txt");
const BLOG_V1: &[u8] =
    include_bytes!("../../../fixtures/news/a06/blobs/regional_blog_mirror_v1.txt");

const REGION: &str = "region-kestrel-archipelago-synth";
const HARBOR_EVENT: &str = "evt-vesper-harbor-channel-hold";
const CX9_EVENT: &str = "evt-cx9-range-calibration";
const REEF_MATTER: &str = "matter-reef-scanner-vpsb-0412";

#[allow(clippy::too_many_arguments)]
fn record<'a>(
    record_id: &'a str,
    version_id: &'a str,
    source_id: &'a str,
    predecessor_version_id: Option<&'a str>,
    superseded_by_version_id: Option<&'a str>,
    availability: AvailabilityState,
    retraction_status: RetractionStatus,
    review_status: ReviewStatus,
    needs_reassessment: bool,
    independent_lineage_root_id: &'a str,
    is_syndicated_reprint: bool,
    raw_bytes: &'a [u8],
    declared_sha256_hex: &'a str,
    expected_char_length: usize,
    passages: &'a [PassageAnchor<'a>],
) -> SourceRecordInput<'a> {
    SourceRecordInput {
        record_id,
        version_id,
        source_id,
        predecessor_version_id,
        superseded_by_version_id,
        availability,
        retraction_status,
        review_status,
        needs_reassessment,
        independent_lineage_root_id,
        is_syndicated_reprint,
        raw_bytes,
        declared_sha256_hex,
        expected_char_length,
        passages,
    }
}

fn check(failures: &mut Vec<String>, label: &str, actual: usize, expected: usize) {
    if actual != expected {
        failures.push(format!("{label}: expected {expected}, got {actual}"));
    }
}

fn print_account(account: &RegionalAccount<'_>) {
    println!(
        "region={} window={}..{} events={} distinct_articles={} distinct_independent_sources={} unanchored_reprint_classes={} legal_matters_spanning_events={:?}",
        account.region_id,
        account.window_start_utc,
        account.window_end_utc,
        account.events.len(),
        account.distinct_articles,
        account.distinct_independent_sources,
        account.unanchored_reprint_classes,
        account.legal_matters_spanning_events,
    );
    println!("category_totals={:?}", account.category_totals);
    println!(
        "development_event_counts={:?}",
        account.development_event_counts
    );
    println!("coverage={:?}", account.coverage);
    for event in &account.events {
        println!(
            "event={} version={} decision={} counts={:?} declared_roots={:?} approximate_location={} uncertainty_m={:?}..{:?} needs_reassessment={}",
            event.event_id,
            event.version_id,
            event.grouping_decision_id,
            event.counts,
            event.declared_lineage_roots,
            event.approximate_location_present,
            event.tightest_location_uncertainty_m,
            event.widest_location_uncertainty_m,
            event.needs_reassessment,
        );
        for entry in &event.entries {
            println!(
                "  entry={} category={:?} kind={:?} record={} speaker={:?} window={:?} movement={} superseded_record={} review={:?} reassess={}",
                entry.candidate_id,
                entry.category,
                entry.development_kind,
                entry.source_record_version_id,
                entry.attributed_speaker,
                entry.window_overlap,
                entry.reports_movement,
                entry.record_superseded,
                entry.review_status,
                entry.needs_reassessment,
            );
            if let Some(purpose) = entry.purpose {
                println!(
                    "    purpose kind={:?} attributed_to={:?} basis={:?} as={:?} passage={} text={:?}",
                    purpose.kind,
                    purpose.attributed_to,
                    purpose.attribution_basis,
                    purpose.category,
                    purpose.passage_id,
                    purpose.text,
                );
            }
        }
        for matter in &event.legal_matters {
            let stages: Vec<String> = matter
                .stages
                .iter()
                .map(|s| format!("{:?}:{}", s.category, s.candidate_id))
                .collect();
            println!(
                "  legal_matter={} stages=[{}] latest_reported={:?} judgment_reported={} chronology_conflict={}",
                matter.matter_id,
                stages.join(", "),
                matter.latest_reported_stage,
                matter.judgment_reported,
                matter.chronology_conflict,
            );
        }
        for conflict in event.conflicts {
            println!(
                "  conflict={} resolution={:?} conflicting={:?} corrected_or_authoritative={:?}",
                conflict.conflict_id,
                conflict.resolution,
                conflict.conflicting_version_ids,
                conflict.corrected_or_authoritative_version_ids,
            );
        }
    }
}

fn main() -> ExitCode {
    let Ok(assessment_limits) = AssessmentLimits::new(32, 32, 16, 32, 32, 16, 16, 16, 65_536)
    else {
        eprintln!("invalid assessment limits");
        return ExitCode::from(1);
    };
    let Ok(limits) = RegionalLimits::new(16, 64, 32, 16, 8, 4_096) else {
        eprintln!("invalid regional limits");
        return ExitCode::from(1);
    };

    let source_ids = [
        "src-pelagic-msd",
        "src-vesper-harbor-coop",
        "src-meridian-wire",
        "src-archipelago-chronicle",
        "src-pelagic-digest",
        "src-vesper-port-safety",
        "src-maritime-adj-panel",
        "src-sable-shore-notebook",
    ];

    let pelagic_passages = [
        PassageAnchor {
            passage_id: "p-pelagic-1",
            language: "en",
            byte_range: 360..588,
            char_range: 360..588,
            quote: "Synthetic acoustic beacon and optical range calibration exercise in fictional Offshore Calibration Box CX-9 (bounding polygon: 21.850N 158.450W; 21.850N 158.950W; 21.400N 158.950W; 21.400N 158.450W; WGS84, surface to 1500m MSL).",
        },
        PassageAnchor {
            passage_id: "p-pelagic-2",
            language: "en",
            byte_range: 823..1062,
            char_range: 823..1062,
            quote: "Commercial shipping lanes and Vesper Harbor approaches on fictional Kestrel Atoll remain open. Vessel movement to Calibration Box CX-9 is a scheduled sensor calibration plan and does not establish an operational blockade or exclusion zone.",
        },
    ];
    let harbor_passages = [
        PassageAnchor {
            passage_id: "p-harbor-1",
            language: "en",
            byte_range: 680..835,
            char_range: 680..835,
            quote: "Four vessels total were logged in the approach sector during the 35-minute interval (1 unlit survey launch, 2 harbor assist tugs, 1 inbound coastal barge).",
        },
        PassageAnchor {
            passage_id: "p-harbor-ja-1",
            language: "ja",
            byte_range: 925..1053,
            char_range: 925..973,
            quote: "「未点灯の高速調査艇が西水路ブイB3付近を通過、19時15分より約35分間、水路誘導を一時待機」",
        },
        PassageAnchor {
            passage_id: "p-harbor-en-trans-1",
            language: "en",
            byte_range: 1076..1224,
            char_range: 996..1144,
            quote: "Unlit high-speed survey launch passed near West Fairway Buoy B3; harbor pilotage held on standby for approximately 35 minutes starting at 19:15 UTC.",
        },
        PassageAnchor {
            passage_id: "p-harbor-kealoha-quote",
            language: "en",
            byte_range: 1300..1463,
            char_range: 1220..1383,
            quote: "Harbor control held inbound pilotage for 35 minutes while tugs escorted the unlit survey craft clear of the buoy line; no range vessels entered the harbor channel.",
        },
    ];
    let wire_v1_passages = [PassageAnchor {
        passage_id: "p-wire-v1-allegation",
        language: "en",
        byte_range: 889..1156,
        char_range: 889..1156,
        quote: "An anonymous dockside contractor alleged that the channel closure was ordered to recover a classified sub-surface test payload tied to the offshore calibration exercise in Calibration Box CX-9. Officials had not responded to the contractor's allegation at press time.",
    }];
    let port_passages = [
        PassageAnchor {
            passage_id: "p-port-detention",
            language: "en",
            byte_range: 453..719,
            char_range: 453..719,
            quote: "Vesper Harbor Enforcement boarded the 14-meter commercial hydrographic survey launch M/V Reef Scanner (Fictional Reg. KA-9941-CS) at Pier 2B at 2024-03-15T20:05:00Z and temporarily detained the vessel master for credential verification and sober-operation screening.",
        },
        PassageAnchor {
            passage_id: "p-port-charges",
            language: "en",
            byte_range: 758..1024,
            char_range: 758..1024,
            quote: "Count 1 (Administrative Charge): Operating a commercial survey vessel in a restricted fairway approach with inoperative navigation lamps and unannounced AIS transponder dropout.\n   - Count 2 (Provisional Charge): Willful obstruction of a commercial pilotage fairway.",
        },
    ];
    let docket_passages = [
        PassageAnchor {
            passage_id: "p-docket-dismissal",
            language: "en",
            byte_range: 357..601,
            char_range: 357..601,
            quote: "Count 2 (Provisional Charge: Willful obstruction of a commercial pilotage fairway) is DISMISSED WITH PREJUDICE following verification of onboard electrical bus logs showing an uncommanded 12V navigation/AIS breaker trip at 2024-03-15T19:09:00Z.",
        },
        PassageAnchor {
            passage_id: "p-docket-civil-penalty",
            language: "en",
            byte_range: 605..792,
            char_range: 605..792,
            quote: "Count 1 (Civil Equipment & Fairway Notice Violation) is SUSTAINED as a non-criminal civil infraction; a $1,250 civil equipment penalty is entered against fictional Coastal Bathymetry LLC.",
        },
    ];

    use AvailabilityState::{Available, Removed};
    use RetractionStatus::{CorrectedByLaterVersion, CorrectionIssued, NotRetracted};
    use ReviewStatus::{Challenged, Reviewed, Superseded};
    let records = [
        record(
            "sr-pelagic-notice-0041",
            "sr-pelagic-notice-0041@v1",
            "src-pelagic-msd",
            None,
            None,
            Available,
            NotRetracted,
            Reviewed,
            false,
            "lineage-root-pelagic-notice-0041",
            false,
            PELAGIC_NOTICE_V1,
            "d123c806bdaf7dd6f4cb5b6f6acf75d89347d301c7f7082266a41eb3723100e2",
            1063,
            &pelagic_passages,
        ),
        record(
            "sr-vesper-harbor-log",
            "sr-vesper-harbor-log@v1",
            "src-vesper-harbor-coop",
            None,
            None,
            Available,
            NotRetracted,
            Reviewed,
            false,
            "lineage-root-vesper-harbor-log",
            false,
            HARBOR_LOG_V1,
            "dfd794a92436f6f3d4b9ce89911b24ac7dbd7d5c3aedac633dda8325b95d19f2",
            1385,
            &harbor_passages,
        ),
        record(
            "sr-meridian-wire-884",
            "sr-meridian-wire-884@v1",
            "src-meridian-wire",
            None,
            Some("sr-meridian-wire-884@v2"),
            Available,
            CorrectedByLaterVersion,
            Superseded,
            false,
            "lineage-root-vesper-harbor-log",
            false,
            WIRE_V1,
            "71d1711b01827bbc4aeb0c8de436915740c801eda1884c1d5704622e0bae9394",
            1157,
            &wire_v1_passages,
        ),
        record(
            "sr-meridian-wire-884",
            "sr-meridian-wire-884@v2",
            "src-meridian-wire",
            Some("sr-meridian-wire-884@v1"),
            None,
            Available,
            CorrectionIssued,
            Reviewed,
            false,
            "lineage-root-vesper-harbor-log",
            false,
            WIRE_V2,
            "09c22c65e113c8450f10c89f5db6b1ca6b39a563202de56281a5dd6f542452af",
            1659,
            &[],
        ),
        record(
            "sr-archipelago-chronicle-884",
            "sr-archipelago-chronicle-884@v1",
            "src-archipelago-chronicle",
            None,
            None,
            Available,
            NotRetracted,
            Challenged,
            true,
            "lineage-root-vesper-harbor-log",
            true,
            TRIBUNE_V1,
            "0aa57f0179675ce54e7ae68836a007bfd47fdb98d7d800a4c2279da43d88d258",
            1074,
            &[],
        ),
        record(
            "sr-pelagic-digest-884",
            "sr-pelagic-digest-884@v1",
            "src-pelagic-digest",
            None,
            None,
            Available,
            NotRetracted,
            Reviewed,
            true,
            "lineage-root-vesper-harbor-log",
            true,
            MARITIME_DAILY_V1,
            "b9744a0a5c2193d3715d8663b402b57d496bc77c199ce6024fb6ede88fd26438",
            957,
            &[],
        ),
        record(
            "sr-vesper-port-safety-0412",
            "sr-vesper-port-safety-0412@v1",
            "src-vesper-port-safety",
            None,
            None,
            Available,
            NotRetracted,
            Reviewed,
            false,
            "lineage-root-vesper-enforcement-0412",
            false,
            PORT_SAFETY_V1,
            "36ee5ebf6240e1f278e7c811f53ebb5fe7d9214918250654490c32bd95426d70",
            1218,
            &port_passages,
        ),
        record(
            "sr-maritime-adj-panel-0412",
            "sr-maritime-adj-panel-0412@v1",
            "src-maritime-adj-panel",
            None,
            None,
            Available,
            NotRetracted,
            Reviewed,
            false,
            "lineage-root-vesper-enforcement-0412",
            false,
            DOCKET_V1,
            "0fe614b291d6076ed31cba1496cbdfcbc635fb55b924967ca2c487f2756e9574",
            960,
            &docket_passages,
        ),
        record(
            "sr-sable-shore-notebook",
            "sr-sable-shore-notebook@v1",
            "src-sable-shore-notebook",
            None,
            None,
            Removed,
            NotRetracted,
            Reviewed,
            false,
            "lineage-root-sable-shore-notebook",
            false,
            BLOG_V1,
            "bc04385fc998a3465416211754e3b4233d70d6a73e22b0b5bb3d857c2bd38b2e",
            762,
            &[],
        ),
    ];

    let lineage_edges = [
        LineageEdgeInput {
            edge_id: "edge-wire-quotes-harbor-log",
            from_version_id: "sr-meridian-wire-884@v1",
            to_version_id: "sr-vesper-harbor-log@v1",
            relation_kind: LineageRelationKind::Quotes,
            preserves_independent_corroboration: false,
        },
        LineageEdgeInput {
            edge_id: "edge-tribune-republishes-wire-v1",
            from_version_id: "sr-archipelago-chronicle-884@v1",
            to_version_id: "sr-meridian-wire-884@v1",
            relation_kind: LineageRelationKind::Republishes,
            preserves_independent_corroboration: false,
        },
        LineageEdgeInput {
            edge_id: "edge-maritime-daily-republishes-wire-v1",
            from_version_id: "sr-pelagic-digest-884@v1",
            to_version_id: "sr-meridian-wire-884@v1",
            relation_kind: LineageRelationKind::Republishes,
            preserves_independent_corroboration: false,
        },
        LineageEdgeInput {
            edge_id: "edge-wire-v2-supersedes-v1",
            from_version_id: "sr-meridian-wire-884@v2",
            to_version_id: "sr-meridian-wire-884@v1",
            relation_kind: LineageRelationKind::Supersedes,
            preserves_independent_corroboration: false,
        },
        LineageEdgeInput {
            edge_id: "edge-docket-updates-port-bulletin",
            from_version_id: "sr-maritime-adj-panel-0412@v1",
            to_version_id: "sr-vesper-port-safety-0412@v1",
            relation_kind: LineageRelationKind::Supersedes,
            preserves_independent_corroboration: false,
        },
        LineageEdgeInput {
            edge_id: "edge-blog-corroborates-harbor-tugs",
            from_version_id: "sr-sable-shore-notebook@v1",
            to_version_id: "sr-vesper-harbor-log@v1",
            relation_kind: LineageRelationKind::Supports,
            preserves_independent_corroboration: true,
        },
    ];

    let base_candidate =
        |candidate_id, source_record_version_id, review_status, needs| EventCandidateInput {
            candidate_id,
            source_record_version_id,
            review_status,
            needs_reassessment: needs,
            movement_establishes_intent_or_mission: false,
        };
    let corpus_candidates = [
        base_candidate(
            "cand-cx9-calibration-plan",
            "sr-pelagic-notice-0041@v1",
            Reviewed,
            false,
        ),
        base_candidate(
            "cand-west-channel-unlit-craft",
            "sr-vesper-harbor-log@v1",
            Reviewed,
            false,
        ),
        base_candidate(
            "cand-kealoha-pilotage-statement",
            "sr-vesper-harbor-log@v1",
            Reviewed,
            false,
        ),
        base_candidate(
            "cand-contractor-payload-allegation",
            "sr-meridian-wire-884@v1",
            Challenged,
            true,
        ),
        base_candidate(
            "cand-reef-scanner-detention",
            "sr-vesper-port-safety-0412@v1",
            Reviewed,
            false,
        ),
        base_candidate(
            "cand-reef-scanner-charges",
            "sr-vesper-port-safety-0412@v1",
            Superseded,
            true,
        ),
        base_candidate(
            "cand-reef-scanner-judgment",
            "sr-maritime-adj-panel-0412@v1",
            Reviewed,
            false,
        ),
    ];

    let corpus = CorpusAssessmentInput {
        source_ids: &source_ids,
        records: &records,
        lineage_edges: &lineage_edges,
        candidates: &corpus_candidates,
        claims: &[],
        invalidations: &[],
        publisher_summaries: &[],
    };

    let harbor_time = ReportedInterval {
        start_utc: "2024-03-15T19:12:00Z",
        end_utc: "2024-03-15T19:47:00Z",
        uncertainty_s: 30,
    };
    let port_time = ReportedInterval {
        start_utc: "2024-03-15T19:12:00Z",
        end_utc: "2024-03-15T20:05:00Z",
        uncertainty_s: 30,
    };
    let point = |uncertainty_m| ReportedLocation {
        basis: LocationBasis::PointWithUncertaintyRadius,
        uncertainty_m: Some(uncertainty_m),
    };

    let regional_candidates = [
        RegionalCandidateInput {
            candidate_id: "cand-cx9-calibration-plan",
            proposed_event_id: CX9_EVENT,
            category: EpistemicCategory::Plan,
            development_kind: DevelopmentKind::Exercise,
            attributed_speaker: None,
            supporting_passage_ids: &["p-pelagic-1", "p-pelagic-2"],
            reported_time: Some(ReportedInterval {
                start_utc: "2024-03-15T18:00:00Z",
                end_utc: "2024-03-16T04:00:00Z",
                uncertainty_s: 60,
            }),
            location: ReportedLocation {
                basis: LocationBasis::Polygon,
                uncertainty_m: Some(500.0),
            },
            reports_movement: true,
            stated_purpose: Some(StatedPurposeInput {
                kind: PurposeKind::Destination,
                text: "Calibration Box CX-9, for a scheduled sensor calibration plan",
                passage_id: "p-pelagic-2",
            }),
            legal_matter_id: None,
        },
        RegionalCandidateInput {
            candidate_id: "cand-west-channel-unlit-craft",
            proposed_event_id: HARBOR_EVENT,
            category: EpistemicCategory::Observation,
            development_kind: DevelopmentKind::TransportDisruption,
            attributed_speaker: None,
            supporting_passage_ids: &["p-harbor-1", "p-harbor-ja-1", "p-harbor-en-trans-1"],
            reported_time: Some(harbor_time),
            location: point(450.0),
            reports_movement: true,
            stated_purpose: None,
            legal_matter_id: None,
        },
        RegionalCandidateInput {
            candidate_id: "cand-kealoha-pilotage-statement",
            proposed_event_id: HARBOR_EVENT,
            category: EpistemicCategory::AttributedStatement,
            development_kind: DevelopmentKind::TransportDisruption,
            attributed_speaker: Some("Capt. M. Kealoha (Fictional Duty Pilot)"),
            supporting_passage_ids: &["p-harbor-kealoha-quote"],
            reported_time: Some(harbor_time),
            location: point(450.0),
            reports_movement: true,
            stated_purpose: None,
            legal_matter_id: None,
        },
        RegionalCandidateInput {
            candidate_id: "cand-contractor-payload-allegation",
            proposed_event_id: HARBOR_EVENT,
            category: EpistemicCategory::Allegation,
            development_kind: DevelopmentKind::TransportDisruption,
            attributed_speaker: Some("Anonymous dockside contractor (fictional)"),
            supporting_passage_ids: &["p-wire-v1-allegation"],
            reported_time: Some(ReportedInterval {
                start_utc: "2024-03-15T19:00:00Z",
                end_utc: "2024-03-15T20:00:00Z",
                uncertainty_s: 1_800,
            }),
            location: ReportedLocation {
                basis: LocationBasis::GazetteerCentroid,
                uncertainty_m: Some(25_000.0),
            },
            reports_movement: false,
            stated_purpose: Some(StatedPurposeInput {
                kind: PurposeKind::Mission,
                text: "recover a classified sub-surface test payload tied to the offshore calibration exercise",
                passage_id: "p-wire-v1-allegation",
            }),
            legal_matter_id: None,
        },
        RegionalCandidateInput {
            candidate_id: "cand-reef-scanner-judgment",
            proposed_event_id: HARBOR_EVENT,
            category: EpistemicCategory::Judgment,
            development_kind: DevelopmentKind::PublicSafetyIncident,
            attributed_speaker: None,
            supporting_passage_ids: &["p-docket-dismissal", "p-docket-civil-penalty"],
            reported_time: Some(ReportedInterval {
                start_utc: "2024-03-18T00:30:00Z",
                end_utc: "2024-03-18T00:30:00Z",
                uncertainty_s: 0,
            }),
            location: point(50.0),
            reports_movement: false,
            stated_purpose: None,
            legal_matter_id: Some(REEF_MATTER),
        },
        RegionalCandidateInput {
            candidate_id: "cand-reef-scanner-charges",
            proposed_event_id: HARBOR_EVENT,
            category: EpistemicCategory::Charge,
            development_kind: DevelopmentKind::PublicSafetyIncident,
            attributed_speaker: None,
            supporting_passage_ids: &["p-port-charges"],
            reported_time: Some(port_time),
            location: point(50.0),
            reports_movement: false,
            stated_purpose: None,
            legal_matter_id: Some(REEF_MATTER),
        },
        RegionalCandidateInput {
            candidate_id: "cand-reef-scanner-detention",
            proposed_event_id: HARBOR_EVENT,
            category: EpistemicCategory::Arrest,
            development_kind: DevelopmentKind::PublicSafetyIncident,
            attributed_speaker: None,
            supporting_passage_ids: &["p-port-detention"],
            reported_time: Some(port_time),
            location: point(50.0),
            reports_movement: false,
            stated_purpose: None,
            legal_matter_id: Some(REEF_MATTER),
        },
    ];

    let harbor_conflicts = [
        EventConflictInput {
            conflict_id: "conf-vessel-count-14-vs-4",
            description: "Wire v1 and syndicated reprints reported 14 vessels in the channel hold; Harbor Watch Log and Wire v2 confirm 4 vessels total (14-meter launch misread as 14 vessels).",
            conflicting_version_ids: &[
                "sr-meridian-wire-884@v1",
                "sr-archipelago-chronicle-884@v1",
                "sr-pelagic-digest-884@v1",
            ],
            corrected_or_authoritative_version_ids: &[
                "sr-vesper-harbor-log@v1",
                "sr-meridian-wire-884@v2",
            ],
            resolution: ConflictResolution::ResolvedByCorrectionHistoryPreserved,
        },
        EventConflictInput {
            conflict_id: "conf-gazetteer-centroid-vs-buoy-b3",
            description: "Wire v1 and syndicated articles use the fictional Kestrel Atoll gazetteer centroid (+/-25 km) while Harbor Log and Port Safety bulletin anchor to Buoy B3 (+/-50-450 m).",
            conflicting_version_ids: &[
                "sr-meridian-wire-884@v1",
                "sr-archipelago-chronicle-884@v1",
            ],
            corrected_or_authoritative_version_ids: &[
                "sr-vesper-harbor-log@v1",
                "sr-vesper-port-safety-0412@v1",
            ],
            resolution: ConflictResolution::BothRetainedWithUncertainty,
        },
    ];
    let events = [
        ProposedEventInput {
            event_id: HARBOR_EVENT,
            version_id: "evt-vesper-harbor-channel-hold@v2",
            region_id: REGION,
            grouping_decision_id: "cluster-rev-2",
            additional_member_version_ids: &[
                "sr-meridian-wire-884@v2",
                "sr-archipelago-chronicle-884@v1",
                "sr-pelagic-digest-884@v1",
                "sr-sable-shore-notebook@v1",
            ],
            conflicts: &harbor_conflicts,
        },
        ProposedEventInput {
            event_id: CX9_EVENT,
            version_id: "evt-cx9-range-calibration@v2",
            region_id: REGION,
            grouping_decision_id: "cluster-rev-2",
            additional_member_version_ids: &[],
            conflicts: &[],
        },
    ];

    let input = RegionalAccountInput {
        region_id: REGION,
        window_start_utc: "2024-03-14T00:00:00Z",
        window_end_utc: "2024-03-18T23:59:59Z",
        events: &events,
        candidates: &regional_candidates,
    };

    let deadline = Instant::now() + Duration::from_secs(5);
    let account = match build_regional_account(&corpus, assessment_limits, &input, limits, || {
        if Instant::now() > deadline {
            Err(StopReason::DeadlineExceeded)
        } else {
            Ok(())
        }
    }) {
        Ok(account) => account,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    };
    print_account(&account);

    // Expected values from fixtures/news/a06/manifest.json (cluster-rev-2).
    let mut failures = Vec::new();
    check(&mut failures, "proposed_events", account.events.len(), 2);
    check(
        &mut failures,
        "distinct_articles",
        account.distinct_articles,
        8,
    );
    check(
        &mut failures,
        "independent_sources",
        account.distinct_independent_sources,
        4,
    );
    for (event_id, articles, versions, independent) in
        [(HARBOR_EVENT, 7, 8, 3), (CX9_EVENT, 1, 1, 1)]
    {
        match account.events.iter().find(|e| e.event_id == event_id) {
            Some(event) => {
                check(
                    &mut failures,
                    &format!("{event_id} articles"),
                    event.counts.articles,
                    articles,
                );
                check(
                    &mut failures,
                    &format!("{event_id} article_versions"),
                    event.counts.article_versions,
                    versions,
                );
                check(
                    &mut failures,
                    &format!("{event_id} independent_sources"),
                    event.counts.independent_sources,
                    independent,
                );
            }
            None => failures.push(format!("{event_id} missing from account")),
        }
    }

    if failures.is_empty() {
        println!("status=ok fixture_counts_match=true");
        ExitCode::SUCCESS
    } else {
        for failure in &failures {
            eprintln!("mismatch: {failure}");
        }
        ExitCode::from(1)
    }
}
