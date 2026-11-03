//! Read-only synthetic `A08` inspection example for `oia_analysis::research_links`
//! (`INTEL-02`, `INTEL-04`, `REASON-03`, `A08`, `#36`).
//!
//! Exercises:
//! 1. Public links across organizations, grants, contracts, facilities, publications, and events,
//!    with `ObservedCoOccurrence` kept separate from substantive funding/contract links.
//! 2. Reversible conflicting-identity resolution (`DisputedIdentity`) over an ambiguous acronym
//!    mention (`"AERO-LAB"`), followed by explicit reversal (`DecisionState::Reversed`) of the
//!    false candidate so the grant link resolves cleanly.
//! 3. Directed circular citation tracing (`CitesPublication` cycle `pub:alpha -> pub:beta -> pub:alpha`).
//! 4. Technical development maturity (`ProposalOrPatentOnly` vs. `ReportedDemonstratedCapability`),
//!    showing that a patent claiming a field demonstration is capped at `PatentSpecificationOnly`
//!    with an explicit caveat, while independent non-circular laboratory test reports are counted
//!    by source lineage group.

use oia_analysis::research_links::{
    AlternateName, AlternateNameKind, ClaimedTechnicalStage, DecisionState, EvidenceOriginClass,
    IdentityCandidateMatch, IdentityMention, LinkEndpoint, PublicEntityKind, PublicLinkRecord,
    PublicLinkRelation, ResearchEntity, ResearchLinkError, ResearchLinkLimits, ResearchLinkRequest,
    ResearchSourceRecord, SourceLifecycleState, TechnicalArtifactKind, TechnicalDevelopmentInput,
    TechnicalEvidenceItem, evaluate_research_links,
};
use oia_domain::text::requires_visible_unicode_escape;
use oia_domain::{ActorId, IdError, OpaqueId, VersionId};

fn escape_display(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if requires_visible_unicode_escape(ch) {
            out.push_str(&format!("\\u{{{:04X}}}", ch as u32));
        } else {
            out.push(ch);
        }
    }
    out
}

#[derive(Debug)]
enum ExampleError {
    Id(IdError),
    Kernel(ResearchLinkError),
}

impl std::fmt::Display for ExampleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Id(err) => write!(f, "{err}"),
            Self::Kernel(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ExampleError {}

impl From<IdError> for ExampleError {
    fn from(err: IdError) -> Self {
        Self::Id(err)
    }
}

impl From<ResearchLinkError> for ExampleError {
    fn from(err: ResearchLinkError) -> Self {
        Self::Kernel(err)
    }
}

#[allow(clippy::too_many_lines)]
fn build_synthetic_a08_request(
    reverse_conflicting_candidate: bool,
) -> Result<ResearchLinkRequest, ExampleError> {
    let analyst = ActorId::new("actor:analyst-01")?;
    let src_award = VersionId::new("ver:src-grant-ledger-v1")?;
    let src_patent = VersionId::new("ver:src-patent-filing-v1")?;
    let src_paper_a = VersionId::new("ver:src-paper-alpha-v1")?;
    let src_paper_b = VersionId::new("ver:src-paper-beta-v1")?;
    let src_lab_test = VersionId::new("ver:src-lab-report-v1")?;

    let sources = vec![
        ResearchSourceRecord {
            source_id: OpaqueId::new("src:grant-ledger")?,
            version_id: src_award.clone(),
            origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            lifecycle: SourceLifecycleState::Current,
            derived_from_versions: Vec::new(),
            citation_label:
                "SYNTHETIC A08 FIXTURE — Public Research Grant Ledger 2026-Q3 (fictional)"
                    .to_owned(),
        },
        ResearchSourceRecord {
            source_id: OpaqueId::new("src:patent-filing")?,
            version_id: src_patent.clone(),
            origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            lifecycle: SourceLifecycleState::Current,
            derived_from_versions: Vec::new(),
            citation_label:
                "SYNTHETIC A08 FIXTURE — Patent Specification US-2026-0999100-A1 (fictional)"
                    .to_owned(),
        },
        ResearchSourceRecord {
            source_id: OpaqueId::new("src:paper-alpha")?,
            version_id: src_paper_a.clone(),
            origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            lifecycle: SourceLifecycleState::Current,
            derived_from_versions: Vec::new(),
            citation_label:
                "SYNTHETIC A08 FIXTURE — Preprint Alpha on Magnetoplasmadynamic Thruster Scaling"
                    .to_owned(),
        },
        ResearchSourceRecord {
            source_id: OpaqueId::new("src:paper-beta")?,
            version_id: src_paper_b.clone(),
            origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            lifecycle: SourceLifecycleState::Current,
            derived_from_versions: Vec::new(),
            citation_label:
                "SYNTHETIC A08 FIXTURE — Survey Beta Citing Preprint Alpha (circular pair)"
                    .to_owned(),
        },
        ResearchSourceRecord {
            source_id: OpaqueId::new("src:lab-report")?,
            version_id: src_lab_test.clone(),
            origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            lifecycle: SourceLifecycleState::Current,
            derived_from_versions: Vec::new(),
            citation_label:
                "SYNTHETIC A08 FIXTURE — Vacuum Chamber Test Report TR-2026-14 (fictional)"
                    .to_owned(),
        },
    ];

    let org_sponsor = OpaqueId::new("ent:org-civic-science-foundation")?;
    let org_aerolab_north = OpaqueId::new("ent:org-aerolab-north")?;
    let org_aerolab_south = OpaqueId::new("ent:org-aerolab-south")?;
    let grant_id = OpaqueId::new("ent:grant-csf-2026-404")?;
    let facility_id = OpaqueId::new("ent:fac-chamber-b")?;
    let pub_alpha = OpaqueId::new("ent:pub-alpha-2026")?;
    let pub_beta = OpaqueId::new("ent:pub-beta-2026")?;
    let pub_lab_tr = OpaqueId::new("ent:pub-lab-tr-2026-14")?;
    let event_symposium = OpaqueId::new("ent:evt-propulsion-symposium-2026")?;

    let entities = vec![
        ResearchEntity {
            entity_id: org_sponsor.clone(),
            kind: PublicEntityKind::Organization,
            primary_name: "Civic Science Foundation".to_owned(),
            alternate_names: vec![AlternateName {
                name: "CSF".to_owned(),
                kind: AlternateNameKind::Acronym,
                source_version: src_award.clone(),
            }],
            defining_source_version: src_award.clone(),
        },
        ResearchEntity {
            entity_id: org_aerolab_north.clone(),
            kind: PublicEntityKind::Organization,
            primary_name: "Aerospace Propulsion Laboratory North".to_owned(),
            alternate_names: vec![
                AlternateName {
                    name: "AERO-LAB".to_owned(),
                    kind: AlternateNameKind::Acronym,
                    source_version: src_award.clone(),
                },
                AlternateName {
                    name: "АэроЛаб-Север".to_owned(),
                    kind: AlternateNameKind::Transliteration {
                        script: "Cyrl".to_owned(),
                    },
                    source_version: src_award.clone(),
                },
            ],
            defining_source_version: src_award.clone(),
        },
        ResearchEntity {
            entity_id: org_aerolab_south.clone(),
            kind: PublicEntityKind::Organization,
            primary_name: "Aero-Lab Diagnostics South LLC".to_owned(),
            alternate_names: vec![AlternateName {
                name: "AERO-LAB".to_owned(),
                kind: AlternateNameKind::Acronym,
                source_version: src_award.clone(),
            }],
            defining_source_version: src_award.clone(),
        },
        ResearchEntity {
            entity_id: grant_id.clone(),
            kind: PublicEntityKind::Grant,
            primary_name: "Grant CSF-2026-404".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: src_award.clone(),
        },
        ResearchEntity {
            entity_id: facility_id.clone(),
            kind: PublicEntityKind::Facility,
            primary_name: "High-Vacuum Test Stand Chamber B".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: src_lab_test.clone(),
        },
        ResearchEntity {
            entity_id: pub_alpha.clone(),
            kind: PublicEntityKind::Publication,
            primary_name: "Preprint Alpha: MPD Scaling".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: src_paper_a.clone(),
        },
        ResearchEntity {
            entity_id: pub_beta.clone(),
            kind: PublicEntityKind::Publication,
            primary_name: "Survey Beta: MPD Review".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: src_paper_b.clone(),
        },
        ResearchEntity {
            entity_id: pub_lab_tr.clone(),
            kind: PublicEntityKind::Publication,
            primary_name: "Chamber B Test Report TR-2026-14".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: src_lab_test.clone(),
        },
        ResearchEntity {
            entity_id: event_symposium.clone(),
            kind: PublicEntityKind::Event,
            primary_name: "2026 Regional Propulsion Symposium".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: src_award.clone(),
        },
    ];

    let mention_aerolab = OpaqueId::new("mention:awardee-aerolab")?;
    let south_decision = if reverse_conflicting_candidate {
        DecisionState::Reversed {
            actor: analyst.clone(),
            reversal_rationale:
                "Grant ledger DUNS/UEI and facility code match Aerospace Propulsion Laboratory North, not Aero-Lab Diagnostics South LLC."
                    .to_owned(),
        }
    } else {
        DecisionState::Proposed {
            rationale: "Shares acronym AERO-LAB in award summary header.".to_owned(),
        }
    };

    let mentions = vec![IdentityMention {
        mention_id: mention_aerolab.clone(),
        verbatim_text: "AERO-LAB (Grant CSF-2026-404 recipient)".to_owned(),
        source_version: src_award.clone(),
        expected_kind: PublicEntityKind::Organization,
        candidates: vec![
            IdentityCandidateMatch {
                candidate_id: OpaqueId::new("cand:aerolab-north")?,
                target_entity_id: org_aerolab_north.clone(),
                matching_basis: "Matches acronym AERO-LAB and Chamber B facility registration."
                    .to_owned(),
                supporting_source_version: src_award.clone(),
                decision: DecisionState::Confirmed {
                    actor: analyst.clone(),
                    rationale: "Confirmed against award ledger UEI and Chamber B operator record."
                        .to_owned(),
                },
            },
            IdentityCandidateMatch {
                candidate_id: OpaqueId::new("cand:aerolab-south")?,
                target_entity_id: org_aerolab_south.clone(),
                matching_basis: "Shares short trade acronym AERO-LAB.".to_owned(),
                supporting_source_version: src_award.clone(),
                decision: south_decision,
            },
        ],
    }];

    let links = vec![
        PublicLinkRecord {
            link_id: OpaqueId::new("link:csf-funds-grant")?,
            from_endpoint: LinkEndpoint::Entity(org_sponsor),
            to_endpoint: LinkEndpoint::Entity(grant_id.clone()),
            relation: PublicLinkRelation::GrantsFundingTo,
            source_version: src_award.clone(),
            verbatim_excerpt: "Civic Science Foundation issued Grant CSF-2026-404.".to_owned(),
            decision: DecisionState::Confirmed {
                actor: analyst.clone(),
                rationale: "Direct award table entry.".to_owned(),
            },
        },
        PublicLinkRecord {
            link_id: OpaqueId::new("link:grant-to-aerolab-mention")?,
            from_endpoint: LinkEndpoint::Entity(grant_id.clone()),
            to_endpoint: LinkEndpoint::Mention(mention_aerolab.clone()),
            relation: PublicLinkRelation::GrantsFundingTo,
            source_version: src_award.clone(),
            verbatim_excerpt: "Grant CSF-2026-404 awarded to AERO-LAB.".to_owned(),
            decision: DecisionState::Confirmed {
                actor: analyst.clone(),
                rationale: "Award recipient line in ledger.".to_owned(),
            },
        },
        PublicLinkRecord {
            link_id: OpaqueId::new("link:north-operates-chamber-b")?,
            from_endpoint: LinkEndpoint::Entity(org_aerolab_north),
            to_endpoint: LinkEndpoint::Entity(facility_id),
            relation: PublicLinkRelation::OperatesFacility,
            source_version: src_lab_test.clone(),
            verbatim_excerpt:
                "Aerospace Propulsion Laboratory North operates High-Vacuum Test Stand Chamber B."
                    .to_owned(),
            decision: DecisionState::Confirmed {
                actor: analyst.clone(),
                rationale: "Facility header in test report TR-2026-14.".to_owned(),
            },
        },
        PublicLinkRecord {
            link_id: OpaqueId::new("link:co-occurrence-symposium")?,
            from_endpoint: LinkEndpoint::Entity(org_aerolab_south),
            to_endpoint: LinkEndpoint::Entity(event_symposium),
            relation: PublicLinkRelation::ObservedCoOccurrence,
            source_version: src_award.clone(),
            verbatim_excerpt:
                "Aero-Lab Diagnostics South LLC was listed on the same attendee sheet as the 2026 Regional Propulsion Symposium."
                    .to_owned(),
            decision: DecisionState::Confirmed {
                actor: analyst.clone(),
                rationale: "Attendee roster co-occurrence only.".to_owned(),
            },
        },
        PublicLinkRecord {
            link_id: OpaqueId::new("link:cite-alpha-to-beta")?,
            from_endpoint: LinkEndpoint::Entity(pub_alpha.clone()),
            to_endpoint: LinkEndpoint::Entity(pub_beta.clone()),
            relation: PublicLinkRelation::CitesPublication,
            source_version: src_paper_a.clone(),
            verbatim_excerpt: "See Survey Beta [Ref 4] for verification of coil efficiency."
                .to_owned(),
            decision: DecisionState::Confirmed {
                actor: analyst.clone(),
                rationale: "Explicit bibliography entry [Ref 4].".to_owned(),
            },
        },
        PublicLinkRecord {
            link_id: OpaqueId::new("link:cite-beta-to-alpha")?,
            from_endpoint: LinkEndpoint::Entity(pub_beta.clone()),
            to_endpoint: LinkEndpoint::Entity(pub_alpha.clone()),
            relation: PublicLinkRelation::CitesPublication,
            source_version: src_paper_b.clone(),
            verbatim_excerpt: "Coil efficiency values are taken from Preprint Alpha [Ref 1]."
                .to_owned(),
            decision: DecisionState::Confirmed {
                actor: analyst.clone(),
                rationale: "Explicit bibliography entry [Ref 1].".to_owned(),
            },
        },
    ];

    let developments = vec![
        TechnicalDevelopmentInput {
            development_id: OpaqueId::new("dev:patent-and-circular-claim")?,
            topic: "Field-deployed pulse-inductive flight demonstrator".to_owned(),
            subject_endpoint: LinkEndpoint::Mention(mention_aerolab.clone()),
            items: vec![
                TechnicalEvidenceItem {
                    item_id: OpaqueId::new("item:patent-claims-flight")?,
                    artifact_kind: TechnicalArtifactKind::PatentApplicationOrGrant,
                    claimed_stage: ClaimedTechnicalStage::FieldOrOperationalDemonstration,
                    publication_entity_id: None,
                    funding_or_contract_ids: vec![grant_id.clone()],
                    source_version: src_patent,
                    verbatim_claim:
                        "Embodiment 3 describes a flight-capable propulsion assembly."
                            .to_owned(),
                    stated_conditions_and_limitations:
                        "Patent specification text only; no empirical test log or instrumentation table."
                            .to_owned(),
                    decision: DecisionState::Confirmed {
                        actor: analyst.clone(),
                        rationale: "Patent text inspected; capped at patent specification.".to_owned(),
                    },
                },
                TechnicalEvidenceItem {
                    item_id: OpaqueId::new("item:circular-paper-claim")?,
                    artifact_kind: TechnicalArtifactKind::LaboratoryTestReport,
                    claimed_stage: ClaimedTechnicalStage::LaboratoryDemonstration,
                    publication_entity_id: Some(pub_alpha),
                    funding_or_contract_ids: Vec::new(),
                    source_version: src_paper_a,
                    verbatim_claim: "Reports 82% coil efficiency citing Survey Beta.".to_owned(),
                    stated_conditions_and_limitations:
                        "Part of a two-paper circular citation loop without independent test data."
                            .to_owned(),
                    decision: DecisionState::Confirmed {
                        actor: analyst.clone(),
                        rationale: "Cites Survey Beta which cites Preprint Alpha back.".to_owned(),
                    },
                },
            ],
        },
        TechnicalDevelopmentInput {
            development_id: OpaqueId::new("dev:chamber-b-vacuum-thruster")?,
            topic: " pulsed magnetoplasmadynamic laboratory thrust pulse in Chamber B"
                .trim()
                .to_owned(),
            subject_endpoint: LinkEndpoint::Mention(mention_aerolab),
            items: vec![TechnicalEvidenceItem {
                item_id: OpaqueId::new("item:chamber-b-tr-14")?,
                artifact_kind: TechnicalArtifactKind::LaboratoryTestReport,
                claimed_stage: ClaimedTechnicalStage::LaboratoryDemonstration,
                publication_entity_id: Some(pub_lab_tr),
                funding_or_contract_ids: vec![grant_id],
                source_version: src_lab_test,
                verbatim_claim:
                    "Measured 42 mN impulse bit over 15 ms pulse at 1.2e-5 Torr in Chamber B."
                        .to_owned(),
                stated_conditions_and_limitations:
                    "Laboratory vacuum chamber stand only; not a flight demonstration."
                        .to_owned(),
                decision: DecisionState::Confirmed {
                    actor: analyst,
                    rationale: "Instrumented test report with stated vacuum chamber bounds."
                        .to_owned(),
                },
            }],
        },
    ];

    Ok(ResearchLinkRequest {
        limits: ResearchLinkLimits::default(),
        sources,
        entities,
        mentions,
        links,
        developments,
    })
}

fn print_report(label: &str, req: &ResearchLinkRequest) -> Result<(), ExampleError> {
    let report = evaluate_research_links(req, || None)?;
    println!("=== {label} (method: {}) ===", report.method_id);
    println!(
        "Caveat: {}",
        escape_display(report.technical_maturity_caveat)
    );
    println!(
        "Links: confirmed_substantive={}, co_occurrence_only={}, disputed_endpoint={}",
        report.confirmed_substantive_link_count,
        report.co_occurrence_link_count,
        report.disputed_endpoint_link_count
    );
    for m in &report.mentions {
        println!(
            "  Mention {}: {:?} (needs_reassessment={})",
            m.mention_id, m.resolution, m.needs_reassessment
        );
    }
    println!(
        "Citation cycles: {} (publications with circular dependency: {:?})",
        report.citation_cycles.len(),
        report.publications_with_circular_dependency
    );
    for c in &report.citation_cycles {
        println!(
            "  Cycle: {:?} via links {:?}",
            c.publication_chain, c.link_ids
        );
    }
    for dev in &report.developments {
        println!(
            "Development {} ({}): overall_maturity={}, resolved_subject={:?}, disputed_subject={}, independent_demo_groups={}, unsupported_demo_claim={}, confirmed_funding={:?}, disputed_funding_mentions={:?}",
            dev.development_id,
            escape_display(&dev.topic),
            dev.overall_maturity,
            dev.resolved_subject_entity_id,
            dev.subject_identity_disputed,
            dev.independent_demonstration_group_count,
            dev.has_unsupported_demonstration_claim,
            dev.confirmed_funding_or_contract_ids,
            dev.disputed_funding_mention_ids
        );
        for item in &dev.items {
            println!(
                "  Item {}: artifact={}, claimed={}, assessed={}, caveats={:?}",
                item.item_id,
                item.artifact_kind,
                item.claimed_stage,
                item.assessed_stage,
                item.caveats
            );
        }
    }
    println!();
    Ok(())
}

fn main() -> Result<(), ExampleError> {
    let disputed_req = build_synthetic_a08_request(false)?;
    print_report(
        "Pass 1: Conflicting identity candidate still Proposed (DisputedIdentity)",
        &disputed_req,
    )?;

    let resolved_req = build_synthetic_a08_request(true)?;
    print_report(
        "Pass 2: Conflicting identity candidate explicitly Reversed with rationale",
        &resolved_req,
    )?;

    Ok(())
}
