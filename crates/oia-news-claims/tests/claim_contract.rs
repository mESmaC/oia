use oia_news_claims::{
    AffectedObjectInput, AssessmentCounts, AssessmentLimits, AvailabilityState,
    ClaimAssessmentState, ClaimCheckError, ClaimInput, CorpusAssessmentInput,
    CorrectionHistoryEntry, DependencyInvalidationInput, DependentObjectKind, EventCandidateInput,
    FaultCategory, LimitKind, PassageAnchor, PassageRef, PublisherSummaryInput,
    QuotationFidelityStatus, RetractionStatus, ReviewStatus, SourceRecordInput, StopReason,
    verify_corpus,
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

struct A06FixtureData {
    source_ids: Vec<&'static str>,
    pelagic_passages: Vec<PassageAnchor<'static>>,
    harbor_passages: Vec<PassageAnchor<'static>>,
    wire_v1_passages: Vec<PassageAnchor<'static>>,
    wire_v2_passages: Vec<PassageAnchor<'static>>,
    tribune_passages: Vec<PassageAnchor<'static>>,
    maritime_passages: Vec<PassageAnchor<'static>>,
    port_passages: Vec<PassageAnchor<'static>>,
    docket_passages: Vec<PassageAnchor<'static>>,
    blog_passages: Vec<PassageAnchor<'static>>,
    candidates: Vec<EventCandidateInput<'static>>,
    claim1_sup: Vec<PassageRef<'static>>,
    claim2_con: Vec<PassageRef<'static>>,
    claim3_con: Vec<PassageRef<'static>>,
    claim4_con: Vec<PassageRef<'static>>,
    claim5_con: Vec<PassageRef<'static>>,
    claim6_sup: Vec<PassageRef<'static>>,
    claim6_con: Vec<PassageRef<'static>>,
    claim7_sup: Vec<PassageRef<'static>>,
    inv1_affected: Vec<AffectedObjectInput<'static>>,
    inv2_affected: Vec<AffectedObjectInput<'static>>,
    wire_corr_history: Vec<CorrectionHistoryEntry<'static>>,
}

impl A06FixtureData {
    fn new() -> Self {
        Self {
            source_ids: vec![
                "src-pelagic-msd",
                "src-vesper-harbor-coop",
                "src-meridian-wire",
                "src-archipelago-chronicle",
                "src-pelagic-digest",
                "src-vesper-port-safety",
                "src-maritime-adj-panel",
                "src-sable-shore-notebook",
            ],
            pelagic_passages: vec![
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
            ],
            harbor_passages: vec![
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
            ],
            wire_v1_passages: vec![
                PassageAnchor {
                    passage_id: "p-wire-v1-14vessels",
                    language: "en",
                    byte_range: 597..766,
                    char_range: 597..766,
                    quote: "According to cooperative watch logs reviewed by this synthetic wire service, 14 vessels were involved in the West Fairway standoff as harbor tugs responded near Buoy B3.",
                },
                PassageAnchor {
                    passage_id: "p-wire-v1-kealoha",
                    language: "en",
                    byte_range: 767..887,
                    char_range: 767..887,
                    quote: "Duty pilot Capt. M. Kealoha said harbor control held inbound pilotage for 35 minutes while the craft was escorted clear.",
                },
                PassageAnchor {
                    passage_id: "p-wire-v1-allegation",
                    language: "en",
                    byte_range: 889..1156,
                    char_range: 889..1156,
                    quote: "An anonymous dockside contractor alleged that the channel closure was ordered to recover a classified sub-surface test payload tied to the offshore calibration exercise in Calibration Box CX-9. Officials had not responded to the contractor's allegation at press time.",
                },
            ],
            wire_v2_passages: vec![
                PassageAnchor {
                    passage_id: "p-wire-v2-correction",
                    language: "en",
                    byte_range: 407..742,
                    char_range: 407..742,
                    quote: "CORRECTION NOTICE: An earlier version of this synthetic dispatch misstated the number of vessels logged in the Vesper Harbor West Fairway due to a transcription error (\"14-meter survey launch\" read as \"14 vessels\"). The watch log recorded 4 vessels total (one 14-meter commercial survey launch, two harbor tugs, and one coastal barge).",
                },
                PassageAnchor {
                    passage_id: "p-wire-v2-withdrawn-allegation",
                    language: "en",
                    byte_range: 889..1101,
                    char_range: 889..1101,
                    quote: "the contractor who alleged a sub-surface payload recovery withdrew the claim after Vesper Port Safety Board enforcement records identified the craft as a private hydrographic survey launch with a failed AIS unit.",
                },
            ],
            tribune_passages: vec![
                PassageAnchor {
                    passage_id: "p-tribune-headline",
                    language: "en",
                    byte_range: 294..381,
                    char_range: 294..381,
                    quote: "HEADLINE: SYNTHETIC FIXTURE -- MARITIME LOCKDOWN HALTS SHIPPING ACROSS KESTREL CORRIDOR",
                },
                PassageAnchor {
                    passage_id: "p-tribune-distorted-quote",
                    language: "en",
                    byte_range: 640..880,
                    char_range: 640..880,
                    quote: "In a dramatic statement, duty pilot Capt. M. Kealoha said \"harbor control confirmed a full corridor lockdown\" after an unidentified craft was intercepted near Buoy B3 during a classified sub-surface recovery mission in Calibration Box CX-9.",
                },
                PassageAnchor {
                    passage_id: "p-tribune-crime-surge",
                    language: "en",
                    byte_range: 882..1073,
                    char_range: 882..1073,
                    quote: "With seven regional news articles published on harbor incidents this week compared to two articles last week, maritime crime and security breaches in the Kestrel Corridor have surged by 350%.",
                },
            ],
            maritime_passages: vec![PassageAnchor {
                passage_id: "p-maritime-daily-reprint",
                language: "en",
                byte_range: 586..847,
                char_range: 586..847,
                quote: "According to cooperative watch logs reviewed by Pacific Wire Service, 14 vessels were involved in the West Fairway hold near Buoy B3. Duty pilot Capt. M. Kealoha stated that harbor control held inbound pilotage for 35 minutes while the craft was escorted clear.",
            }],
            port_passages: vec![
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
                PassageAnchor {
                    passage_id: "p-port-no-military",
                    language: "en",
                    byte_range: 1040..1217,
                    char_range: 1040..1217,
                    quote: "Inspection confirmed M/V Reef Scanner is a privately contracted bathymetric survey launch. No range tenders or sub-surface payloads were involved in the harbor channel incident.",
                },
            ],
            docket_passages: vec![
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
            ],
            blog_passages: vec![PassageAnchor {
                passage_id: "p-blog-sighting",
                language: "en",
                byte_range: 403..604,
                char_range: 403..604,
                quote: "Readers on the fictional Sable Shore headland reported seeing two harbor tugs idling near the Vesper Harbor entrance buoy around 9:20 a.m. KST today while an inbound barge waited outside the reef line.",
            }],
            candidates: vec![
                EventCandidateInput {
                    candidate_id: "cand-cx9-calibration-plan",
                    source_record_version_id: "sr-pelagic-notice-0041@v1",
                    review_status: ReviewStatus::Reviewed,
                    needs_reassessment: false,
                    movement_establishes_intent_or_mission: false,
                },
                EventCandidateInput {
                    candidate_id: "cand-west-channel-unlit-craft",
                    source_record_version_id: "sr-vesper-harbor-log@v1",
                    review_status: ReviewStatus::Reviewed,
                    needs_reassessment: false,
                    movement_establishes_intent_or_mission: false,
                },
                EventCandidateInput {
                    candidate_id: "cand-kealoha-pilotage-statement",
                    source_record_version_id: "sr-vesper-harbor-log@v1",
                    review_status: ReviewStatus::Reviewed,
                    needs_reassessment: false,
                    movement_establishes_intent_or_mission: false,
                },
                EventCandidateInput {
                    candidate_id: "cand-contractor-payload-allegation",
                    source_record_version_id: "sr-meridian-wire-884@v1",
                    review_status: ReviewStatus::Challenged,
                    needs_reassessment: true,
                    movement_establishes_intent_or_mission: false,
                },
                EventCandidateInput {
                    candidate_id: "cand-reef-scanner-detention",
                    source_record_version_id: "sr-vesper-port-safety-0412@v1",
                    review_status: ReviewStatus::Reviewed,
                    needs_reassessment: false,
                    movement_establishes_intent_or_mission: false,
                },
                EventCandidateInput {
                    candidate_id: "cand-reef-scanner-charges",
                    source_record_version_id: "sr-vesper-port-safety-0412@v1",
                    review_status: ReviewStatus::Superseded,
                    needs_reassessment: true,
                    movement_establishes_intent_or_mission: false,
                },
                EventCandidateInput {
                    candidate_id: "cand-reef-scanner-judgment",
                    source_record_version_id: "sr-maritime-adj-panel-0412@v1",
                    review_status: ReviewStatus::Reviewed,
                    needs_reassessment: false,
                    movement_establishes_intent_or_mission: false,
                },
            ],
            claim1_sup: vec![
                PassageRef {
                    source_record_version_id: "sr-vesper-harbor-log@v1",
                    passage_id: "p-harbor-1",
                },
                PassageRef {
                    source_record_version_id: "sr-vesper-harbor-log@v1",
                    passage_id: "p-harbor-ja-1",
                },
                PassageRef {
                    source_record_version_id: "sr-vesper-port-safety-0412@v1",
                    passage_id: "p-port-detention",
                },
                PassageRef {
                    source_record_version_id: "sr-meridian-wire-884@v2",
                    passage_id: "p-wire-v2-correction",
                },
            ],
            claim2_con: vec![
                PassageRef {
                    source_record_version_id: "sr-vesper-harbor-log@v1",
                    passage_id: "p-harbor-1",
                },
                PassageRef {
                    source_record_version_id: "sr-meridian-wire-884@v2",
                    passage_id: "p-wire-v2-correction",
                },
            ],
            claim3_con: vec![
                PassageRef {
                    source_record_version_id: "sr-vesper-port-safety-0412@v1",
                    passage_id: "p-port-no-military",
                },
                PassageRef {
                    source_record_version_id: "sr-meridian-wire-884@v2",
                    passage_id: "p-wire-v2-withdrawn-allegation",
                },
            ],
            claim4_con: vec![
                PassageRef {
                    source_record_version_id: "sr-vesper-harbor-log@v1",
                    passage_id: "p-harbor-kealoha-quote",
                },
                PassageRef {
                    source_record_version_id: "sr-meridian-wire-884@v1",
                    passage_id: "p-wire-v1-kealoha",
                },
                PassageRef {
                    source_record_version_id: "sr-pelagic-notice-0041@v1",
                    passage_id: "p-pelagic-2",
                },
            ],
            claim5_con: vec![
                PassageRef {
                    source_record_version_id: "sr-vesper-port-safety-0412@v1",
                    passage_id: "p-port-no-military",
                },
                PassageRef {
                    source_record_version_id: "sr-maritime-adj-panel-0412@v1",
                    passage_id: "p-docket-civil-penalty",
                },
            ],
            claim6_sup: vec![PassageRef {
                source_record_version_id: "sr-vesper-port-safety-0412@v1",
                passage_id: "p-port-charges",
            }],
            claim6_con: vec![PassageRef {
                source_record_version_id: "sr-maritime-adj-panel-0412@v1",
                passage_id: "p-docket-dismissal",
            }],
            claim7_sup: vec![PassageRef {
                source_record_version_id: "sr-vesper-harbor-log@v1",
                passage_id: "p-harbor-1",
            }],
            inv1_affected: vec![
                AffectedObjectInput {
                    object_id: "sr-archipelago-chronicle-884@v1",
                    object_kind: DependentObjectKind::SourceRecord,
                    status_set_needs_reassessment: true,
                    prior_issued_version_preserved: true,
                },
                AffectedObjectInput {
                    object_id: "sr-pelagic-digest-884@v1",
                    object_kind: DependentObjectKind::SourceRecord,
                    status_set_needs_reassessment: true,
                    prior_issued_version_preserved: true,
                },
                AffectedObjectInput {
                    object_id: "cand-contractor-payload-allegation",
                    object_kind: DependentObjectKind::EventCandidate,
                    status_set_needs_reassessment: true,
                    prior_issued_version_preserved: true,
                },
                AffectedObjectInput {
                    object_id: "report-vesper-harbor-brief@v1",
                    object_kind: DependentObjectKind::Report,
                    status_set_needs_reassessment: true,
                    prior_issued_version_preserved: true,
                },
            ],
            inv2_affected: vec![
                AffectedObjectInput {
                    object_id: "cand-reef-scanner-charges",
                    object_kind: DependentObjectKind::EventCandidate,
                    status_set_needs_reassessment: true,
                    prior_issued_version_preserved: true,
                },
                AffectedObjectInput {
                    object_id: "claim-06-willful-fairway-obstruction@v2",
                    object_kind: DependentObjectKind::Claim,
                    status_set_needs_reassessment: true,
                    prior_issued_version_preserved: true,
                },
                AffectedObjectInput {
                    object_id: "report-vesper-harbor-brief@v1",
                    object_kind: DependentObjectKind::Report,
                    status_set_needs_reassessment: true,
                    prior_issued_version_preserved: true,
                },
            ],
            wire_corr_history: vec![CorrectionHistoryEntry {
                original_version_id: "sr-meridian-wire-884@v1",
                corrected_version_id: "sr-meridian-wire-884@v2",
                corrected_at: "2024-03-16T03:40:00Z",
                summary: "Corrected vessel count from 14 to 4 and noted withdrawal of uncorroborated contractor allegation.",
            }],
        }
    }

    fn build_records(&self) -> Vec<SourceRecordInput<'_>> {
        vec![
            SourceRecordInput {
                record_id: "sr-pelagic-notice-0041",
                version_id: "sr-pelagic-notice-0041@v1",
                source_id: "src-pelagic-msd",
                predecessor_version_id: None,
                superseded_by_version_id: None,
                availability: AvailabilityState::Available,
                retraction_status: RetractionStatus::NotRetracted,
                review_status: ReviewStatus::Reviewed,
                needs_reassessment: false,
                independent_lineage_root_id: "lineage-root-pelagic-notice-0041",
                is_syndicated_reprint: false,
                raw_bytes: PELAGIC_NOTICE_V1,
                expected_sha256_hex: "d123c806bdaf7dd6f4cb5b6f6acf75d89347d301c7f7082266a41eb3723100e2",
                expected_char_length: 1063,
                passages: &self.pelagic_passages,
            },
            SourceRecordInput {
                record_id: "sr-vesper-harbor-log",
                version_id: "sr-vesper-harbor-log@v1",
                source_id: "src-vesper-harbor-coop",
                predecessor_version_id: None,
                superseded_by_version_id: None,
                availability: AvailabilityState::Available,
                retraction_status: RetractionStatus::NotRetracted,
                review_status: ReviewStatus::Reviewed,
                needs_reassessment: false,
                independent_lineage_root_id: "lineage-root-vesper-harbor-log",
                is_syndicated_reprint: false,
                raw_bytes: HARBOR_LOG_V1,
                expected_sha256_hex: "dfd794a92436f6f3d4b9ce89911b24ac7dbd7d5c3aedac633dda8325b95d19f2",
                expected_char_length: 1385,
                passages: &self.harbor_passages,
            },
            SourceRecordInput {
                record_id: "sr-meridian-wire-884",
                version_id: "sr-meridian-wire-884@v1",
                source_id: "src-meridian-wire",
                predecessor_version_id: None,
                superseded_by_version_id: Some("sr-meridian-wire-884@v2"),
                availability: AvailabilityState::Available,
                retraction_status: RetractionStatus::CorrectedByLaterVersion,
                review_status: ReviewStatus::Superseded,
                needs_reassessment: false,
                independent_lineage_root_id: "lineage-root-vesper-harbor-log",
                is_syndicated_reprint: false,
                raw_bytes: WIRE_V1,
                expected_sha256_hex: "71d1711b01827bbc4aeb0c8de436915740c801eda1884c1d5704622e0bae9394",
                expected_char_length: 1157,
                passages: &self.wire_v1_passages,
            },
            SourceRecordInput {
                record_id: "sr-meridian-wire-884",
                version_id: "sr-meridian-wire-884@v2",
                source_id: "src-meridian-wire",
                predecessor_version_id: Some("sr-meridian-wire-884@v1"),
                superseded_by_version_id: None,
                availability: AvailabilityState::Available,
                retraction_status: RetractionStatus::CorrectionIssued,
                review_status: ReviewStatus::Reviewed,
                needs_reassessment: false,
                independent_lineage_root_id: "lineage-root-vesper-harbor-log",
                is_syndicated_reprint: false,
                raw_bytes: WIRE_V2,
                expected_sha256_hex: "09c22c65e113c8450f10c89f5db6b1ca6b39a563202de56281a5dd6f542452af",
                expected_char_length: 1659,
                passages: &self.wire_v2_passages,
            },
            SourceRecordInput {
                record_id: "sr-archipelago-chronicle-884",
                version_id: "sr-archipelago-chronicle-884@v1",
                source_id: "src-archipelago-chronicle",
                predecessor_version_id: None,
                superseded_by_version_id: None,
                availability: AvailabilityState::Available,
                retraction_status: RetractionStatus::NotRetracted,
                review_status: ReviewStatus::Challenged,
                needs_reassessment: true,
                independent_lineage_root_id: "lineage-root-vesper-harbor-log",
                is_syndicated_reprint: true,
                raw_bytes: TRIBUNE_V1,
                expected_sha256_hex: "0aa57f0179675ce54e7ae68836a007bfd47fdb98d7d800a4c2279da43d88d258",
                expected_char_length: 1074,
                passages: &self.tribune_passages,
            },
            SourceRecordInput {
                record_id: "sr-pelagic-digest-884",
                version_id: "sr-pelagic-digest-884@v1",
                source_id: "src-pelagic-digest",
                predecessor_version_id: None,
                superseded_by_version_id: None,
                availability: AvailabilityState::Available,
                retraction_status: RetractionStatus::NotRetracted,
                review_status: ReviewStatus::Reviewed,
                needs_reassessment: true,
                independent_lineage_root_id: "lineage-root-vesper-harbor-log",
                is_syndicated_reprint: true,
                raw_bytes: MARITIME_DAILY_V1,
                expected_sha256_hex: "b9744a0a5c2193d3715d8663b402b57d496bc77c199ce6024fb6ede88fd26438",
                expected_char_length: 957,
                passages: &self.maritime_passages,
            },
            SourceRecordInput {
                record_id: "sr-vesper-port-safety-0412",
                version_id: "sr-vesper-port-safety-0412@v1",
                source_id: "src-vesper-port-safety",
                predecessor_version_id: None,
                superseded_by_version_id: None,
                availability: AvailabilityState::Available,
                retraction_status: RetractionStatus::NotRetracted,
                review_status: ReviewStatus::Reviewed,
                needs_reassessment: false,
                independent_lineage_root_id: "lineage-root-vesper-enforcement-0412",
                is_syndicated_reprint: false,
                raw_bytes: PORT_SAFETY_V1,
                expected_sha256_hex: "36ee5ebf6240e1f278e7c811f53ebb5fe7d9214918250654490c32bd95426d70",
                expected_char_length: 1218,
                passages: &self.port_passages,
            },
            SourceRecordInput {
                record_id: "sr-maritime-adj-panel-0412",
                version_id: "sr-maritime-adj-panel-0412@v1",
                source_id: "src-maritime-adj-panel",
                predecessor_version_id: None,
                superseded_by_version_id: None,
                availability: AvailabilityState::Available,
                retraction_status: RetractionStatus::NotRetracted,
                review_status: ReviewStatus::Reviewed,
                needs_reassessment: false,
                independent_lineage_root_id: "lineage-root-vesper-enforcement-0412",
                is_syndicated_reprint: false,
                raw_bytes: DOCKET_V1,
                expected_sha256_hex: "0fe614b291d6076ed31cba1496cbdfcbc635fb55b924967ca2c487f2756e9574",
                expected_char_length: 960,
                passages: &self.docket_passages,
            },
            SourceRecordInput {
                record_id: "sr-sable-shore-notebook",
                version_id: "sr-sable-shore-notebook@v1",
                source_id: "src-sable-shore-notebook",
                predecessor_version_id: None,
                superseded_by_version_id: None,
                availability: AvailabilityState::Removed,
                retraction_status: RetractionStatus::NotRetracted,
                review_status: ReviewStatus::Reviewed,
                needs_reassessment: false,
                independent_lineage_root_id: "lineage-root-sable-shore-notebook",
                is_syndicated_reprint: false,
                raw_bytes: BLOG_V1,
                expected_sha256_hex: "bc04385fc998a3465416211754e3b4233d70d6a73e22b0b5bb3d857c2bd38b2e",
                expected_char_length: 762,
                passages: &self.blog_passages,
            },
        ]
    }

    fn build_claims(&self) -> Vec<ClaimInput<'_>> {
        vec![
            ClaimInput {
                claim_id: "claim-01-four-vessels-35min-hold",
                version_id: "claim-01-four-vessels-35min-hold@v1",
                source_record_version_id: "sr-vesper-harbor-log@v1",
                passage_id: "p-harbor-1",
                headline_passage_id: None,
                proposition: "Four vessels were present in the Vesper Harbor West Fairway during a 35-minute inbound pilotage hold caused by an unlit 14-meter commercial survey launch.",
                claimant: "Vesper Harbor Pilotage Cooperative Watch Officer (fictional)",
                quotes_claim_without_endorsing: false,
                is_opinion_not_factual_premise: false,
                assessment_at_publication: ClaimAssessmentState::Supported,
                current_assessment: ClaimAssessmentState::Supported,
                review_status: ReviewStatus::Reviewed,
                availability: AvailabilityState::Available,
                needs_reassessment: false,
                fault_categories: &[],
                supporting_evidence: &self.claim1_sup,
                contrary_evidence: &[],
                criteria_and_reasoning: "Directly recorded in primary harbor watch log (English and Japanese VHF log) and independently corroborated by Vesper Port Safety boarding record SYNTH-VPSB-0412.",
            },
            ClaimInput {
                claim_id: "claim-02-wire-v1-fourteen-vessels",
                version_id: "claim-02-wire-v1-fourteen-vessels@v2",
                source_record_version_id: "sr-meridian-wire-884@v1",
                passage_id: "p-wire-v1-14vessels",
                headline_passage_id: None,
                proposition: "Fourteen vessels were involved in a West Fairway standoff near Buoy B3.",
                claimant: "Pacific Wire Service Synthetic Fixture (v1)",
                quotes_claim_without_endorsing: false,
                is_opinion_not_factual_premise: false,
                assessment_at_publication: ClaimAssessmentState::Contradicted,
                current_assessment: ClaimAssessmentState::Contradicted,
                review_status: ReviewStatus::Reviewed,
                availability: AvailabilityState::Available,
                needs_reassessment: false,
                fault_categories: &[FaultCategory::NumericalError],
                supporting_evidence: &[],
                contrary_evidence: &self.claim2_con,
                criteria_and_reasoning: "Numerical transcription error conflating '14-meter commercial survey launch' from SYNTH-HBP-LOG-0019 with '14 vessels'; formally corrected in SYNTH-PWS-0884 v2.",
            },
            ClaimInput {
                claim_id: "claim-03-contractor-payload-allegation",
                version_id: "claim-03-contractor-payload-allegation@v2",
                source_record_version_id: "sr-meridian-wire-884@v1",
                passage_id: "p-wire-v1-allegation",
                headline_passage_id: None,
                proposition: "An anonymous dockside contractor alleged the channel hold was ordered to recover a classified sub-surface test payload linked to Calibration Box CX-9.",
                claimant: "Anonymous dockside contractor (attributed by Wire v1)",
                quotes_claim_without_endorsing: true,
                is_opinion_not_factual_premise: false,
                assessment_at_publication: ClaimAssessmentState::UnsupportedByAvailableEvidence,
                current_assessment: ClaimAssessmentState::UnsupportedByAvailableEvidence,
                review_status: ReviewStatus::Reviewed,
                availability: AvailabilityState::Available,
                needs_reassessment: false,
                fault_categories: &[],
                supporting_evidence: &[],
                contrary_evidence: &self.claim3_con,
                criteria_and_reasoning: "Wire v1 attributed the statement as an unconfirmed allegation without endorsing it. At publication time it lacked corroborating evidence (unsupported_by_available_evidence is distinct from proven false by lack of corroboration alone); in v2 the speaker withdrew the allegation after Port Safety inspection.",
            },
            ClaimInput {
                claim_id: "claim-04-tribune-blockade-and-distorted-quote",
                version_id: "claim-04-tribune-blockade-and-distorted-quote@v1",
                source_record_version_id: "sr-archipelago-chronicle-884@v1",
                passage_id: "p-tribune-distorted-quote",
                headline_passage_id: Some("p-tribune-headline"),
                proposition: "A maritime lockdown halted shipping across the Kestrel Corridor and Capt. M. Kealoha stated that 'harbor control confirmed a full corridor lockdown'.",
                claimant: "Archipelago Chronicle Wire Desk (Synthetic)",
                quotes_claim_without_endorsing: false,
                is_opinion_not_factual_premise: false,
                assessment_at_publication: ClaimAssessmentState::MateriallyMisleading,
                current_assessment: ClaimAssessmentState::MateriallyMisleading,
                review_status: ReviewStatus::Reviewed,
                availability: AvailabilityState::Available,
                needs_reassessment: false,
                fault_categories: &[
                    FaultCategory::HeadlineCaptionMismatch,
                    FaultCategory::DistortedQuotation,
                    FaultCategory::FactualContradiction,
                    FaultCategory::UnsupportedCertainty,
                ],
                supporting_evidence: &[],
                contrary_evidence: &self.claim4_con,
                criteria_and_reasoning: "Fabricates a direct quote ('harbor control confirmed a full corridor lockdown') that appears neither in the underlying wire dispatch nor in the harbor log, where Capt. Kealoha explicitly stated 'no range vessels entered the harbor channel'.",
            },
            ClaimInput {
                claim_id: "claim-05-tribune-350pct-crime-surge",
                version_id: "claim-05-tribune-350pct-crime-surge@v1",
                source_record_version_id: "sr-archipelago-chronicle-884@v1",
                passage_id: "p-tribune-crime-surge",
                headline_passage_id: None,
                proposition: "Maritime crime and security breaches in the Kestrel Corridor surged by 350% this week based on seven published news articles versus two articles last week.",
                claimant: "Archipelago Chronicle Wire Desk (Synthetic)",
                quotes_claim_without_endorsing: false,
                is_opinion_not_factual_premise: false,
                assessment_at_publication: ClaimAssessmentState::MateriallyMisleading,
                current_assessment: ClaimAssessmentState::MateriallyMisleading,
                review_status: ReviewStatus::Reviewed,
                availability: AvailabilityState::Available,
                needs_reassessment: false,
                fault_categories: &[
                    FaultCategory::SelectiveComparison,
                    FaultCategory::NumericalError,
                    FaultCategory::MateriallyMissingContext,
                    FaultCategory::NewsVolumeEquatedWithCrimeRate,
                ],
                supporting_evidence: &[],
                contrary_evidence: &self.claim5_con,
                criteria_and_reasoning: "Violates NEWS-03 and NEWS-05 by equating syndicated article count (multiple reprints of one non-criminal equipment malfunction) with incident frequency and maritime crime rate.",
            },
            ClaimInput {
                claim_id: "claim-06-willful-fairway-obstruction",
                version_id: "claim-06-willful-fairway-obstruction@v2",
                source_record_version_id: "sr-vesper-port-safety-0412@v1",
                passage_id: "p-port-charges",
                headline_passage_id: None,
                proposition: "M/V Reef Scanner willfully obstructed a commercial pilotage fairway (Provisional Count 2).",
                claimant: "Vesper Port Safety Board (initial provisional citation)",
                quotes_claim_without_endorsing: false,
                is_opinion_not_factual_premise: false,
                assessment_at_publication: ClaimAssessmentState::Disputed,
                current_assessment: ClaimAssessmentState::Contradicted,
                review_status: ReviewStatus::Reviewed,
                availability: AvailabilityState::Available,
                needs_reassessment: true,
                fault_categories: &[],
                supporting_evidence: &self.claim6_sup,
                contrary_evidence: &self.claim6_con,
                criteria_and_reasoning: "Separates publication-time knowledge (provisional administrative charge filed 2024-03-16, disputed by vessel operator) from later adjudication outcome (dismissed with prejudice on 2024-03-18 after onboard electrical logs proved an uncommanded 12V breaker trip).",
            },
            ClaimInput {
                claim_id: "claim-07-shoreline-barge-waiting-distance",
                version_id: "claim-07-shoreline-barge-waiting-distance@v1",
                source_record_version_id: "sr-sable-shore-notebook@v1",
                passage_id: "p-blog-sighting",
                headline_passage_id: None,
                proposition: "Observers on the Sable Shore headland saw two harbor tugs idling near the Vesper Harbor entrance buoy around 09:20 KST while an inbound barge waited outside the reef line.",
                claimant: "Sable Shore Community Notebook readers (fictional)",
                quotes_claim_without_endorsing: true,
                is_opinion_not_factual_premise: false,
                assessment_at_publication: ClaimAssessmentState::Unresolved,
                current_assessment: ClaimAssessmentState::Unresolved,
                review_status: ReviewStatus::Reviewed,
                availability: AvailabilityState::Removed,
                needs_reassessment: false,
                fault_categories: &[],
                supporting_evidence: &self.claim7_sup,
                contrary_evidence: &[],
                criteria_and_reasoning: "Source page returned HTTP 410 Gone on 2024-03-16T06:00:00Z during a notebook migration without a retraction notice (availability: removed, retraction_status: not_retracted). General tug/barge presence aligns with Harbor Watch Log, but observer vantage accuracy remains unresolved.",
            },
        ]
    }

    fn build_invalidations(&self) -> Vec<DependencyInvalidationInput<'_>> {
        vec![
            DependencyInvalidationInput {
                invalidation_id: "inv-wire-v2-correction",
                trigger_version_id: "sr-meridian-wire-884@v2",
                trigger_reason: "source_correction",
                affected_objects: &self.inv1_affected,
            },
            DependencyInvalidationInput {
                invalidation_id: "inv-docket-final-judgment",
                trigger_version_id: "sr-maritime-adj-panel-0412@v1",
                trigger_reason: "case_outcome_update",
                affected_objects: &self.inv2_affected,
            },
        ]
    }

    fn build_publisher_summaries(&self) -> Vec<PublisherSummaryInput<'_>> {
        vec![
            PublisherSummaryInput {
                summary_id: "pubsum-meridian-wire-2024w11",
                source_id: "src-meridian-wire",
                publisher_name: "Pacific Wire Service (Synthetic Fixture)",
                window_start_utc: "2024-03-14T00:00:00Z",
                window_end_utc: "2024-03-18T23:59:59Z",
                sample_size: 2,
                evaluated_source_record_versions: &[
                    "sr-meridian-wire-884@v1",
                    "sr-meridian-wire-884@v2",
                ],
                sampling_method: "complete_case_corpus_within_window",
                assessment_counts: AssessmentCounts {
                    supported: 0,
                    contradicted: 1,
                    unsupported_by_available_evidence: 1,
                    materially_misleading: 0,
                    disputed: 0,
                    unresolved: 0,
                },
                unresolved_assessment_count: 0,
                correction_history: &self.wire_corr_history,
                uncorrected_upstream_dependencies: &[],
                article_count_in_window: 2,
                distinct_underlying_incidents: 1,
                represents_incident_frequency_or_crime_rate: false,
                methodology_note: "Article and revision counts reflect wire publication activity on a single civil navigation equipment incident and must not be interpreted as regional incident frequency or crime rate (NEWS-05).",
            },
            PublisherSummaryInput {
                summary_id: "pubsum-archipelago-chronicle-2024w11",
                source_id: "src-archipelago-chronicle",
                publisher_name: "Archipelago Chronicle Online (Synthetic)",
                window_start_utc: "2024-03-14T00:00:00Z",
                window_end_utc: "2024-03-18T23:59:59Z",
                sample_size: 1,
                evaluated_source_record_versions: &["sr-archipelago-chronicle-884@v1"],
                sampling_method: "complete_case_corpus_within_window",
                assessment_counts: AssessmentCounts {
                    supported: 0,
                    contradicted: 0,
                    unsupported_by_available_evidence: 0,
                    materially_misleading: 2,
                    disputed: 0,
                    unresolved: 0,
                },
                unresolved_assessment_count: 0,
                correction_history: &[],
                uncorrected_upstream_dependencies: &["sr-meridian-wire-884@v1"],
                article_count_in_window: 1,
                distinct_underlying_incidents: 1,
                represents_incident_frequency_or_crime_rate: false,
                methodology_note: "Syndicated wire adaptation only; publisher claim equating regional article counts with a 350% maritime crime surge is flagged as materially_misleading under NEWS-03/NEWS-05.",
            },
            PublisherSummaryInput {
                summary_id: "pubsum-sable-shore-notebook-2024w11",
                source_id: "src-sable-shore-notebook",
                publisher_name: "Sable Shore Community Notebook (Synthetic)",
                window_start_utc: "2024-03-14T00:00:00Z",
                window_end_utc: "2024-03-18T23:59:59Z",
                sample_size: 1,
                evaluated_source_record_versions: &["sr-sable-shore-notebook@v1"],
                sampling_method: "complete_case_corpus_within_window",
                assessment_counts: AssessmentCounts {
                    supported: 0,
                    contradicted: 0,
                    unsupported_by_available_evidence: 0,
                    materially_misleading: 0,
                    disputed: 0,
                    unresolved: 1,
                },
                unresolved_assessment_count: 1,
                correction_history: &[],
                uncorrected_upstream_dependencies: &[],
                article_count_in_window: 1,
                distinct_underlying_incidents: 1,
                represents_incident_frequency_or_crime_rate: false,
                methodology_note: "Single community post later removed (HTTP 410) without retraction; retained snapshot is not an incident-rate metric.",
            },
        ]
    }
}

fn default_limits() -> AssessmentLimits {
    AssessmentLimits::new(32, 32, 16, 32, 16, 16, 16, 65_536).unwrap_or_else(|_| unreachable!())
}

#[test]
fn verifies_a06_claims_distorted_quotes_and_independent_roots() {
    let fixture = A06FixtureData::new();
    let records = fixture.build_records();
    let claims = fixture.build_claims();
    let invalidations = fixture.build_invalidations();
    let summaries = fixture.build_publisher_summaries();

    let input = CorpusAssessmentInput {
        source_ids: &fixture.source_ids,
        records: &records,
        candidates: &fixture.candidates,
        claims: &claims,
        invalidations: &invalidations,
        publisher_summaries: &summaries,
    };

    let report = verify_corpus(&input, default_limits(), || Ok(()))
        .unwrap_or_else(|err| panic!("expected A06 corpus to pass: {err}"));

    assert_eq!(report.validated_source_count(), 8);
    assert_eq!(report.validated_record_count(), 9);
    assert_eq!(report.validated_passage_count(), 21);
    assert_eq!(report.validated_invalidation_count(), 2);
    assert_eq!(report.validated_publisher_summary_count(), 3);
    assert_eq!(report.verified_claims().len(), 7);

    // Claim 1 cites 4 passages across 3 records, but 2 records share lineage-root-vesper-harbor-log,
    // so independent_supporting_root_count must collapse to 2.
    let c1 = report
        .verified_claims()
        .iter()
        .find(|c| c.claim_id == "claim-01-four-vessels-35min-hold")
        .unwrap_or_else(|| unreachable!());
    assert_eq!(c1.independent_supporting_root_count, 2);
    assert_eq!(
        c1.quotation_status,
        QuotationFidelityStatus::NoVerbatimQuoteInPassage
    );

    // Claim 3 is an attributed allegation ("quotes_claim_without_endorsing") that remains
    // UnsupportedByAvailableEvidence rather than automatically Contradicted.
    let c3 = report
        .verified_claims()
        .iter()
        .find(|c| c.claim_id == "claim-03-contractor-payload-allegation")
        .unwrap_or_else(|| unreachable!());
    assert!(c3.quotes_claim_without_endorsing);
    assert_eq!(
        c3.current_assessment,
        ClaimAssessmentState::UnsupportedByAvailableEvidence
    );

    // Claim 4 fabricates a direct quote ("harbor control confirmed a full corridor lockdown")
    // and is verified as DistortedQuoteDetectedAndFlagged.
    let c4 = report
        .verified_claims()
        .iter()
        .find(|c| c.claim_id == "claim-04-tribune-blockade-and-distorted-quote")
        .unwrap_or_else(|| unreachable!());
    assert_eq!(
        c4.quotation_status,
        QuotationFidelityStatus::DistortedQuoteDetectedAndFlagged
    );

    // Claim 6 separates publication-time Disputed from post-adjudication Contradicted.
    let c6 = report
        .verified_claims()
        .iter()
        .find(|c| c.claim_id == "claim-06-willful-fairway-obstruction")
        .unwrap_or_else(|| unreachable!());
    assert!(c6.temporal_assessment_shifted);
    assert_eq!(c6.assessment_at_publication, ClaimAssessmentState::Disputed);
    assert_eq!(c6.current_assessment, ClaimAssessmentState::Contradicted);

    // Claim 7 comes from a removed page (HTTP 410) and stays Unresolved + Removed, not retracted.
    let c7 = report
        .verified_claims()
        .iter()
        .find(|c| c.claim_id == "claim-07-shoreline-barge-waiting-distance")
        .unwrap_or_else(|| unreachable!());
    assert_eq!(c7.availability, AvailabilityState::Removed);
    assert_eq!(c7.current_assessment, ClaimAssessmentState::Unresolved);
}

#[test]
fn rejects_removed_page_auto_retracted_and_unfounded_contradiction() {
    let fixture = A06FixtureData::new();
    let mut records = fixture.build_records();
    let claims = fixture.build_claims();
    let invalidations = fixture.build_invalidations();
    let summaries = fixture.build_publisher_summaries();

    // Mutate the removed blog record to Retracted -> must fail RemovedPageAutoRetracted
    for rec in &mut records {
        if rec.availability == AvailabilityState::Removed {
            rec.retraction_status = RetractionStatus::Retracted;
        }
    }
    let bad_removed_input = CorpusAssessmentInput {
        source_ids: &fixture.source_ids,
        records: &records,
        candidates: &fixture.candidates,
        claims: &claims,
        invalidations: &invalidations,
        publisher_summaries: &summaries,
    };
    assert_eq!(
        verify_corpus(&bad_removed_input, default_limits(), || Ok(())),
        Err(ClaimCheckError::RemovedPageAutoRetracted {
            record_version_id: "sr-sable-shore-notebook@v1".to_owned(),
        })
    );

    // Restore records and test marking an uncorroborated claim Contradicted with no contrary evidence
    let clean_records = fixture.build_records();
    let mut bad_claims = fixture.build_claims();
    for clm in &mut bad_claims {
        if clm.claim_id == "claim-07-shoreline-barge-waiting-distance" {
            clm.current_assessment = ClaimAssessmentState::Contradicted;
        }
    }
    let bad_contradicted_input = CorpusAssessmentInput {
        source_ids: &fixture.source_ids,
        records: &clean_records,
        candidates: &fixture.candidates,
        claims: &bad_claims,
        invalidations: &invalidations,
        publisher_summaries: &summaries,
    };
    assert!(matches!(
        verify_corpus(&bad_contradicted_input, default_limits(), || Ok(())),
        Err(ClaimCheckError::UnsupportedAutomaticallyFalse {
            claim_id,
            ..
        }) if claim_id == "claim-07-shoreline-barge-waiting-distance"
    ));

    // Also verify that an unflagged distorted quote is rejected
    let mut unflagged_quote_claims = fixture.build_claims();
    for clm in &mut unflagged_quote_claims {
        if clm.claim_id == "claim-04-tribune-blockade-and-distorted-quote" {
            clm.fault_categories = &[FaultCategory::HeadlineCaptionMismatch];
        }
    }
    let unflagged_quote_input = CorpusAssessmentInput {
        source_ids: &fixture.source_ids,
        records: &clean_records,
        candidates: &fixture.candidates,
        claims: &unflagged_quote_claims,
        invalidations: &invalidations,
        publisher_summaries: &summaries,
    };
    assert_eq!(
        verify_corpus(&unflagged_quote_input, default_limits(), || Ok(())),
        Err(ClaimCheckError::DistortedQuoteNotFlagged {
            claim_id: "claim-04-tribune-blockade-and-distorted-quote".to_owned(),
            quoted_text: "harbor control confirmed a full corridor lockdown".to_owned(),
        })
    );
}

#[test]
fn rejects_unpropagated_corrections_and_publisher_volume_or_sample_violations() {
    let fixture = A06FixtureData::new();
    let mut records = fixture.build_records();
    let claims = fixture.build_claims();
    let invalidations = fixture.build_invalidations();
    let summaries = fixture.build_publisher_summaries();

    // Clear needs_reassessment on a dependent syndicated record after Wire v2 correction
    for rec in &mut records {
        if rec.version_id == "sr-archipelago-chronicle-884@v1" {
            rec.needs_reassessment = false;
        }
    }
    let unpropagated_input = CorpusAssessmentInput {
        source_ids: &fixture.source_ids,
        records: &records,
        candidates: &fixture.candidates,
        claims: &claims,
        invalidations: &invalidations,
        publisher_summaries: &summaries,
    };
    assert!(matches!(
        verify_corpus(&unpropagated_input, default_limits(), || Ok(())),
        Err(ClaimCheckError::UnpropagatedCorrection {
            invalidation_id,
            object_id,
            ..
        }) if invalidation_id == "inv-wire-v2-correction"
            && object_id == "sr-archipelago-chronicle-884@v1"
    ));

    // Equate news volume with crime rate in a publisher summary -> must fail VolumeEquatedWithCrimeRate
    let clean_records = fixture.build_records();
    let mut bad_summaries = fixture.build_publisher_summaries();
    for ps in &mut bad_summaries {
        if ps.summary_id == "pubsum-archipelago-chronicle-2024w11" {
            ps.represents_incident_frequency_or_crime_rate = true;
        }
    }
    let bad_volume_input = CorpusAssessmentInput {
        source_ids: &fixture.source_ids,
        records: &clean_records,
        candidates: &fixture.candidates,
        claims: &claims,
        invalidations: &invalidations,
        publisher_summaries: &bad_summaries,
    };
    assert_eq!(
        verify_corpus(&bad_volume_input, default_limits(), || Ok(())),
        Err(ClaimCheckError::VolumeEquatedWithCrimeRate {
            summary_id: "pubsum-archipelago-chronicle-2024w11".to_owned(),
        })
    );

    // Mismatch sample_size in publisher summary -> must fail PublisherSummarySampleMismatch
    let mut bad_sample_summaries = fixture.build_publisher_summaries();
    for ps in &mut bad_sample_summaries {
        if ps.summary_id == "pubsum-meridian-wire-2024w11" {
            ps.sample_size = 5;
        }
    }
    let bad_sample_input = CorpusAssessmentInput {
        source_ids: &fixture.source_ids,
        records: &clean_records,
        candidates: &fixture.candidates,
        claims: &claims,
        invalidations: &invalidations,
        publisher_summaries: &bad_sample_summaries,
    };
    assert!(matches!(
        verify_corpus(&bad_sample_input, default_limits(), || Ok(())),
        Err(ClaimCheckError::PublisherSummarySampleMismatch {
            summary_id,
            ..
        }) if summary_id == "pubsum-meridian-wire-2024w11"
    ));
}

#[test]
fn enforces_multibyte_anchors_budgets_and_cooperative_stop() {
    let mut fixture = A06FixtureData::new();
    // Corrupt the multibyte Japanese passage byte_range so it splits a 3-byte UTF-8 character
    if let Some(ja_passage) = fixture
        .harbor_passages
        .iter_mut()
        .find(|p| p.passage_id == "p-harbor-ja-1")
    {
        ja_passage.byte_range = 925..1052;
    }
    let records = fixture.build_records();
    let claims = fixture.build_claims();
    let invalidations = fixture.build_invalidations();
    let summaries = fixture.build_publisher_summaries();
    let bad_anchor_input = CorpusAssessmentInput {
        source_ids: &fixture.source_ids,
        records: &records,
        candidates: &fixture.candidates,
        claims: &claims,
        invalidations: &invalidations,
        publisher_summaries: &summaries,
    };
    assert_eq!(
        verify_corpus(&bad_anchor_input, default_limits(), || Ok(())),
        Err(ClaimCheckError::InvalidPassageAnchor {
            record_version_id: "sr-vesper-harbor-log@v1".to_owned(),
            passage_id: "p-harbor-ja-1".to_owned(),
            reason: "byte_range splits a multi-byte UTF-8 codepoint",
        })
    );

    // Zero limit rejection
    assert_eq!(
        AssessmentLimits::new(0, 1, 1, 1, 1, 1, 1, 1),
        Err(ClaimCheckError::ZeroLimit(LimitKind::Sources))
    );

    // Tight claim budget rejection
    let clean_fixture = A06FixtureData::new();
    let clean_records = clean_fixture.build_records();
    let clean_claims = clean_fixture.build_claims();
    let clean_invalidations = clean_fixture.build_invalidations();
    let clean_summaries = clean_fixture.build_publisher_summaries();
    let clean_input = CorpusAssessmentInput {
        source_ids: &clean_fixture.source_ids,
        records: &clean_records,
        candidates: &clean_fixture.candidates,
        claims: &clean_claims,
        invalidations: &clean_invalidations,
        publisher_summaries: &clean_summaries,
    };
    let tight_limits =
        AssessmentLimits::new(32, 32, 16, 3, 16, 16, 16, 65_536).unwrap_or_else(|_| unreachable!());
    assert_eq!(
        verify_corpus(&clean_input, tight_limits, || Ok(())),
        Err(ClaimCheckError::BudgetExceeded {
            kind: LimitKind::Claims,
            actual: 7,
            limit: 3,
        })
    );

    // Cooperative stop rejection at every callback step
    for stop_step in 0..45 {
        let mut calls = 0usize;
        let res = verify_corpus(&clean_input, default_limits(), || {
            let current = calls;
            calls += 1;
            if current == stop_step {
                Err(StopReason::Cancelled)
            } else {
                Ok(())
            }
        });
        assert_eq!(res, Err(ClaimCheckError::Stopped(StopReason::Cancelled)));
    }
}
