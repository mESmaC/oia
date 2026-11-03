//! Targeted contract tests for the personal public exposure and owned-infrastructure
//! audit kernel (`#47`, `PRIVACY-02`, `A18`).
//!
//! Uses only synthetic personas, reserved domains (`example.com`, `example.org`, `example.net`,
//! `.invalid`, `.test`), and documentation IP ranges (`192.0.2.0/24`, `2001:db8::/32`).

use oia_analysis::exposure_audit::{
    ACCOUNT_MATCH_UNCERTAINTY_NOTICE, AccountMatchInput, AccountMatchReviewInput,
    AccountMatchSignal, AuditTimePrecision, AuditTimestamp, DeclaredAuditScope, DnsRecordType,
    ExposedContactKind, ExposureAuditError, ExposureAuditLimits, ExposureCitationAnchor,
    InfrastructureArtifactKind, InfrastructureEntryInput, MatchAssessmentState, MatchSignalKind,
    ObservationTimeEnvelope, OutboundQueryTarget, OutboundSourceQueryCandidate, OwnershipBasis,
    PersonalExposureAudit, PublicProfileInput, ScopeRevisionInput, SourceCitation,
    SourceCoverageRecord, SourceQueryCoverage,
};
use oia_domain::collaboration::{
    AccessDecision, AccessDenialReason, AccessSurface, CaseRole, ClearanceGrant,
};
use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

fn utc_instant(unix_ms: i64) -> AuditTimestamp {
    AuditTimestamp {
        unix_ms,
        precision: AuditTimePrecision::ExactInstant {
            original_tz_label: "UTC".to_owned(),
        },
    }
}

/// Protects `PRIVACY-02` and `A18` scope boundaries, out-of-scope pivot refusal, explicit scope
/// revision, time-bounded `AuthorizedAssessmentScope`, and collaboration access-policy integration:
/// - `DeclaredAuditScope` requires at least one valid domain or handle root and normalizes case/trailing dots.
/// - Subdomain matching enforces a strict dot boundary (`telemetry.example.org` is covered by `example.org`,
///   whereas lookalike `myexample.org` is rejected with `DomainOutsideDeclaredScope`).
/// - Discovered candidate accounts or linked domains outside the declared scope are refused on
///   outbound source queries (`HandleOutsideDeclaredScope` / `DomainOutsideDeclaredScope`) and cannot be used
///   as the primary anchor for further account matching (`PrimaryProfileMustBeInDeclaredScope`), preventing
///   automatic pivoting from one person's handle to third parties' accounts.
/// - Widening the scope requires an explicit `revise_declared_scope` transition with an ownership/authorization
///   basis and rationale, retaining prior scope versions in `prior_scopes()`.
/// - `prepare_read_only_source_query` blocks any outbound query that attaches private local notes/hypotheses,
///   exports the full multi-target scope manifest, or mutates remote state, and `evaluate_surface_access`
///   delegates to `oia_domain::collaboration::evaluate_proposed_access_decision` (denying `PublicPublication`).
#[test]
fn declared_scope_refuses_out_of_scope_expansion_and_requires_explicit_revision_to_widen()
-> Result<(), Box<dyn std::error::Error>> {
    let owner = ActorId::new("actor:synthetic-owner")?;
    let limits = ExposureAuditLimits::new(512, 4, 4, 8, 8, 4, 8, 8)?;

    assert_eq!(
        DeclaredAuditScope::new(
            OpaqueId::new("scope:empty")?,
            VersionId::new("ver:s0")?,
            1_000,
            OwnershipBasis::SelfOwnedPersonalScope {
                owner_actor: owner.clone(),
                verification_note: "Empty targets test.".to_owned(),
            },
            Vec::new(),
            Vec::new(),
            limits,
        ),
        Err(ExposureAuditError::EmptyDeclaredScope)
    );

    let scope = DeclaredAuditScope::new(
        OpaqueId::new("scope:owned")?,
        VersionId::new("ver:scope-1")?,
        1_000,
        OwnershipBasis::SelfOwnedPersonalScope {
            owner_actor: owner.clone(),
            verification_note: "SECRET-LOCAL-OWNERSHIP-NOTE".to_owned(),
        },
        vec!["Example.ORG.".to_owned()],
        vec!["Fictional_Handle_1".to_owned()],
        limits,
    )?;

    assert!(scope.covers_domain("example.org"));
    assert!(scope.covers_domain("telemetry.example.org"));
    assert!(!scope.covers_domain("myexample.org"));
    assert!(!scope.covers_domain("example.org.evil.invalid"));
    assert!(scope.covers_handle("fictional_handle_1"));

    let debug_scope = format!("{scope:?}");
    assert!(!debug_scope.contains("SECRET-LOCAL-OWNERSHIP-NOTE"));
    assert!(debug_scope.contains("<redacted-local-note>"));

    let mut audit = PersonalExposureAudit::new(
        OpaqueId::new("audit:scope-test")?,
        SchemaVersion::V1,
        scope,
        limits,
    )?;

    // Proposed access decision integration (#38): PublicPublication is always denied for local exposure audits.
    assert_eq!(
        audit.evaluate_surface_access(
            CaseRole::Owner,
            ClearanceGrant::full(),
            AccessSurface::PublicPublication,
        ),
        Err(ExposureAuditError::AccessDenied(
            AccessDenialReason::UnpublishedDraftRestricted
        ))
    );
    assert_eq!(
        audit.evaluate_surface_access(
            CaseRole::Owner,
            ClearanceGrant::full(),
            AccessSurface::PrivateExport,
        ),
        Ok(AccessDecision::AllowFull)
    );

    // Register an in-scope profile and two discovered external profiles.
    audit.register_public_profile(PublicProfileInput {
        profile_id: OpaqueId::new("prof:in-scope")?,
        version_id: VersionId::new("ver:prof-in-1")?,
        origin: AccountOrigin::Simulated,
        service_name: "SyntheticGit".to_owned(),
        handle: "fictional_handle_1".to_owned(),
        display_name: None,
        bio_excerpt: None,
        listed_website_domain: Some("example.org".to_owned()),
        exposed_contact_values: Vec::new(),
        claimed_in_declared_scope: true,
        citation: SourceCitation {
            source_id: OpaqueId::new("source:git")?,
            source_version: VersionId::new("ver:git-1")?,
            anchor: ExposureCitationAnchor::Utf8ByteRange(0..32),
        },
        timestamps: ObservationTimeEnvelope {
            observed_or_valid_from: Some(utc_instant(1_500)),
            published_or_issued_at: None,
            retrieved_at: utc_instant(1_600),
            audited_at: utc_instant(1_700),
        },
    })?;

    audit.register_public_profile(PublicProfileInput {
        profile_id: OpaqueId::new("prof:external-candidate-a")?,
        version_id: VersionId::new("ver:prof-ext-a")?,
        origin: AccountOrigin::Simulated,
        service_name: "SyntheticForum".to_owned(),
        handle: "fictional_handle_1_alt".to_owned(),
        display_name: None,
        bio_excerpt: None,
        listed_website_domain: Some("example.net".to_owned()),
        exposed_contact_values: Vec::new(),
        claimed_in_declared_scope: false,
        citation: SourceCitation {
            source_id: OpaqueId::new("source:forum")?,
            source_version: VersionId::new("ver:forum-1")?,
            anchor: ExposureCitationAnchor::Utf8ByteRange(0..48),
        },
        timestamps: ObservationTimeEnvelope {
            observed_or_valid_from: None,
            published_or_issued_at: None,
            retrieved_at: utc_instant(1_650),
            audited_at: utc_instant(1_750),
        },
    })?;

    audit.register_public_profile(PublicProfileInput {
        profile_id: OpaqueId::new("prof:third-party-b")?,
        version_id: VersionId::new("ver:prof-ext-b")?,
        origin: AccountOrigin::Simulated,
        service_name: "OtherService".to_owned(),
        handle: "unrelated_third_party".to_owned(),
        display_name: None,
        bio_excerpt: None,
        listed_website_domain: None,
        exposed_contact_values: Vec::new(),
        claimed_in_declared_scope: false,
        citation: SourceCitation {
            source_id: OpaqueId::new("source:other")?,
            source_version: VersionId::new("ver:other-1")?,
            anchor: ExposureCitationAnchor::Utf8ByteRange(0..48),
        },
        timestamps: ObservationTimeEnvelope {
            observed_or_valid_from: None,
            published_or_issued_at: None,
            retrieved_at: utc_instant(1_660),
            audited_at: utc_instant(1_760),
        },
    })?;

    // Refusal 1: A discovered candidate handle (`fictional_handle_1_alt`) or linked domain (`example.net`)
    // outside `DeclaredAuditScope` cannot be fetched/queried via `prepare_read_only_source_query`.
    assert_eq!(
        audit.prepare_read_only_source_query(OutboundSourceQueryCandidate {
            source_id: OpaqueId::new("source:forum")?,
            target: OutboundQueryTarget::DeclaredHandle("fictional_handle_1_alt".to_owned()),
            requested_at_unix_ms: 1_800,
            attaches_private_notes_or_hypotheses: false,
            exports_full_scope_manifest: false,
            mutates_remote_account_or_dns: false,
        }),
        Err(ExposureAuditError::HandleOutsideDeclaredScope)
    );
    assert_eq!(
        audit.prepare_read_only_source_query(OutboundSourceQueryCandidate {
            source_id: OpaqueId::new("source:dns")?,
            target: OutboundQueryTarget::DeclaredDomain("example.net".to_owned()),
            requested_at_unix_ms: 1_800,
            attaches_private_notes_or_hypotheses: false,
            exports_full_scope_manifest: false,
            mutates_remote_account_or_dns: false,
        }),
        Err(ExposureAuditError::DomainOutsideDeclaredScope {
            candidate_domain: "example.net".to_owned(),
        })
    );

    // Refusal 2: Pivoting from an external candidate profile (`prof:external-candidate-a`) to match
    // a third-party profile (`prof:third-party-b`) is refused (`PrimaryProfileMustBeInDeclaredScope`).
    assert_eq!(
        audit.propose_account_match(AccountMatchInput {
            match_id: OpaqueId::new("match:illegal-pivot")?,
            version_id: VersionId::new("ver:match-pivot")?,
            primary_profile_id: OpaqueId::new("prof:external-candidate-a")?,
            candidate_profile_id: OpaqueId::new("prof:third-party-b")?,
            uncalibrated_heuristic_score: 0.75,
            matching_signals: vec![AccountMatchSignal {
                kind: MatchSignalKind::SimilarHandlePattern,
                description: "Attempted transitive pivot outside declared scope.".to_owned(),
                citation: SourceCitation {
                    source_id: OpaqueId::new("source:forum")?,
                    source_version: VersionId::new("ver:forum-1")?,
                    anchor: ExposureCitationAnchor::Utf8ByteRange(0..16),
                },
            }],
            contradicting_signals: Vec::new(),
            ambiguity_explanation: "Transitive pivot test.".to_owned(),
            proposed_at: utc_instant(1_850),
        }),
        Err(ExposureAuditError::PrimaryProfileMustBeInDeclaredScope {
            profile_id: OpaqueId::new("prof:external-candidate-a")?,
        })
    );

    // Reject out-of-scope domain and out-of-scope certificate SAN in infrastructure history.
    assert_eq!(
        audit.record_infrastructure_entry(InfrastructureEntryInput {
            entry_id: OpaqueId::new("infra:bad-dns")?,
            version_id: VersionId::new("ver:bad-dns-1")?,
            origin: AccountOrigin::Simulated,
            domain: "example.net".to_owned(),
            artifact: InfrastructureArtifactKind::DnsRecord {
                record_type: DnsRecordType::A,
                record_value: "192.0.2.10".to_owned(),
                ttl_seconds: Some(300),
            },
            citation: SourceCitation {
                source_id: OpaqueId::new("source:dns")?,
                source_version: VersionId::new("ver:dns-1")?,
                anchor: ExposureCitationAnchor::Utf8ByteRange(0..10),
            },
            timestamps: ObservationTimeEnvelope {
                observed_or_valid_from: Some(utc_instant(2_000)),
                published_or_issued_at: None,
                retrieved_at: utc_instant(2_100),
                audited_at: utc_instant(2_200),
            },
        }),
        Err(ExposureAuditError::DomainOutsideDeclaredScope {
            candidate_domain: "example.net".to_owned(),
        })
    );

    assert_eq!(
        audit.record_infrastructure_entry(InfrastructureEntryInput {
            entry_id: OpaqueId::new("infra:cert-bad-san")?,
            version_id: VersionId::new("ver:cert-bad-san-1")?,
            origin: AccountOrigin::Simulated,
            domain: "example.org".to_owned(),
            artifact: InfrastructureArtifactKind::CertificateHistory {
                certificate_sha256_fingerprint: "a".repeat(64),
                serial_number: "01:02".to_owned(),
                issuer_common_name: "Synthetic Test CA".to_owned(),
                subject_common_name: "example.org".to_owned(),
                subject_alt_names: vec![
                    "api.example.org".to_owned(),
                    "unrelated.example.net".to_owned(),
                ],
                not_before_unix_ms: 1_000,
                not_after_unix_ms: 5_000,
            },
            citation: SourceCitation {
                source_id: OpaqueId::new("source:ct")?,
                source_version: VersionId::new("ver:ct-1")?,
                anchor: ExposureCitationAnchor::Utf8ByteRange(0..20),
            },
            timestamps: ObservationTimeEnvelope {
                observed_or_valid_from: Some(utc_instant(1_000)),
                published_or_issued_at: Some(utc_instant(1_010)),
                retrieved_at: utc_instant(2_000),
                audited_at: utc_instant(2_100),
            },
        }),
        Err(ExposureAuditError::DomainOutsideDeclaredScope {
            candidate_domain: "unrelated.example.net".to_owned(),
        })
    );

    // Explicit scope revision: widen DeclaredAuditScope to include `example.net` with recorded basis and rationale.
    let revised_scope = audit.revise_declared_scope(ScopeRevisionInput {
        expected_scope_version: VersionId::new("ver:scope-1")?,
        new_scope_version: VersionId::new("ver:scope-2")?,
        revised_at_unix_ms: 2_500,
        ownership_basis: OwnershipBasis::SelfOwnedPersonalScope {
            owner_actor: owner,
            verification_note: "Verified secondary domain example.net via DNS TXT.".to_owned(),
        },
        declared_root_domains: vec!["example.org".to_owned(), "example.net".to_owned()],
        declared_handles: vec!["fictional_handle_1".to_owned()],
        revision_rationale:
            "Added secondary personal domain example.net after confirming ownership.".to_owned(),
    })?;
    assert_eq!(
        revised_scope.predecessor_scope_version(),
        Some(&VersionId::new("ver:scope-1")?)
    );
    assert_eq!(audit.prior_scopes().len(), 1);

    // Now querying `example.net` succeeds cleanly when no private notes or full manifest are attached.
    let clean_query = audit.prepare_read_only_source_query(OutboundSourceQueryCandidate {
        source_id: OpaqueId::new("source:ct")?,
        target: OutboundQueryTarget::DeclaredDomain("API.Example.NET.".to_owned()),
        requested_at_unix_ms: 2_600,
        attaches_private_notes_or_hypotheses: false,
        exports_full_scope_manifest: false,
        mutates_remote_account_or_dns: false,
    })?;
    assert_eq!(clean_query.normalized_target(), "api.example.net");
    assert!(clean_query.is_domain_query());

    // Block outbound source queries that would leak private local analysis or full scope manifests.
    assert!(matches!(
        audit.prepare_read_only_source_query(OutboundSourceQueryCandidate {
            source_id: OpaqueId::new("source:ct")?,
            target: OutboundQueryTarget::DeclaredDomain("api.example.net".to_owned()),
            requested_at_unix_ms: 2_600,
            attaches_private_notes_or_hypotheses: true,
            exports_full_scope_manifest: false,
            mutates_remote_account_or_dns: false,
        }),
        Err(ExposureAuditError::PrivateAssessmentLeakBlocked(_))
    ));

    Ok(())
}

/// Protects `PRIVACY-02`, `A18`, and `AGENTS.md` §2 epistemic, authorization-window, and provenance rules:
/// - `AuthorizedAssessmentScope` records who authorized it, what they authorized, and `[valid_from_unix_ms, valid_until_unix_ms]`,
///   and rejects actions outside that window (`AuthorizationExpiredOrNotYetValid`).
/// - Even a `1.0` `uncalibrated_heuristic_score` starts in `MatchAssessmentState::ProposedCandidate`
///   and `establishes_automated_identity()` is always `false`.
/// - Ambiguous account matches can be reviewed as `AmbiguousRetained`, and later rejected as
///   `UserRejectedDifferentEntity` while keeping the rejected candidate, its evidence signals, and its
///   append-only review history intact for later inspection.
/// - Updating/superseding an upstream `source_version` flags dependent profiles, account matches,
///   and infrastructure entries `needs_reassessment = true` without erasing prior reviews.
#[test]
fn authorized_window_ambiguous_match_retention_and_source_reassessment()
-> Result<(), Box<dyn std::error::Error>> {
    let auditor = ActorId::new("actor:synthetic-auditor")?;
    let limits = ExposureAuditLimits::default();

    let scope = DeclaredAuditScope::new(
        OpaqueId::new("scope:auth-01")?,
        VersionId::new("ver:scope-v1")?,
        10_000,
        OwnershipBasis::AuthorizedAssessmentScope {
            auditor_actor: auditor.clone(),
            authorizing_party: "Synthetic Research Institute".to_owned(),
            authorization_reference: "AUTH-2026-11-SYNTH".to_owned(),
            authorized_scope_summary:
                "Authorized exposure review of example.com and handle synth_unit_ops.".to_owned(),
            valid_from_unix_ms: 10_000,
            valid_until_unix_ms: 20_000,
            verification_note: "Signed authorization fixture in local vault.".to_owned(),
        },
        vec!["example.com".to_owned()],
        vec!["synth_unit_ops".to_owned()],
        limits,
    )?;

    let mut audit = PersonalExposureAudit::new(
        OpaqueId::new("audit:match-lifecycle")?,
        SchemaVersion::V1,
        scope,
        limits,
    )?;

    // Action after `valid_until_unix_ms` (20_000) is rejected with `AuthorizationExpiredOrNotYetValid`.
    assert_eq!(
        audit.prepare_read_only_source_query(OutboundSourceQueryCandidate {
            source_id: OpaqueId::new("source:ct")?,
            target: OutboundQueryTarget::DeclaredDomain("example.com".to_owned()),
            requested_at_unix_ms: 25_000,
            attaches_private_notes_or_hypotheses: false,
            exports_full_scope_manifest: false,
            mutates_remote_account_or_dns: false,
        }),
        Err(ExposureAuditError::AuthorizationExpiredOrNotYetValid {
            action_unix_ms: 25_000,
            valid_from_unix_ms: 10_000,
            valid_until_unix_ms: 20_000,
        })
    );

    let src_v1 = VersionId::new("ver:src-directory-v1")?;
    let src_v2 = VersionId::new("ver:src-directory-v2")?;

    audit.register_public_profile(PublicProfileInput {
        profile_id: OpaqueId::new("prof:declared-main")?,
        version_id: VersionId::new("ver:prof-1")?,
        origin: AccountOrigin::Simulated,
        service_name: "SyntheticCodeHost".to_owned(),
        handle: "synth_unit_ops".to_owned(),
        display_name: Some("Synthetic Unit Ops".to_owned()),
        bio_excerpt: None,
        listed_website_domain: Some("example.com".to_owned()),
        exposed_contact_values: Vec::new(),
        claimed_in_declared_scope: true,
        citation: SourceCitation {
            source_id: OpaqueId::new("source:codehost")?,
            source_version: VersionId::new("ver:src-codehost-v1")?,
            anchor: ExposureCitationAnchor::Utf8ByteRange(0..64),
        },
        timestamps: ObservationTimeEnvelope {
            observed_or_valid_from: Some(utc_instant(11_000)),
            published_or_issued_at: None,
            retrieved_at: utc_instant(12_000),
            audited_at: utc_instant(12_100),
        },
    })?;

    audit.register_public_profile(PublicProfileInput {
        profile_id: OpaqueId::new("prof:external-ambiguous")?,
        version_id: VersionId::new("ver:prof-2")?,
        origin: AccountOrigin::Simulated,
        service_name: "SyntheticDirectory".to_owned(),
        handle: "synth_unit_ops".to_owned(),
        display_name: Some("Unit Operations Mirror".to_owned()),
        bio_excerpt: Some("Mirroring public notices.".to_owned()),
        listed_website_domain: None,
        exposed_contact_values: vec!["contact@example.com".to_owned()],
        claimed_in_declared_scope: false,
        citation: SourceCitation {
            source_id: OpaqueId::new("source:directory")?,
            source_version: src_v1.clone(),
            anchor: ExposureCitationAnchor::StructuredRecordField {
                record_locator: "entry/99".to_owned(),
                field_name: "profile".to_owned(),
            },
        },
        timestamps: ObservationTimeEnvelope {
            observed_or_valid_from: None,
            published_or_issued_at: None,
            retrieved_at: utc_instant(12_200),
            audited_at: utc_instant(12_300),
        },
    })?;

    // Even with uncalibrated_heuristic_score == 1.0, state must be ProposedCandidate and never auto-confirmed.
    let match_id = OpaqueId::new("match:unit-ops-1")?;
    let proposed = audit.propose_account_match(AccountMatchInput {
        match_id: match_id.clone(),
        version_id: VersionId::new("ver:m1")?,
        primary_profile_id: OpaqueId::new("prof:declared-main")?,
        candidate_profile_id: OpaqueId::new("prof:external-ambiguous")?,
        uncalibrated_heuristic_score: 1.0,
        matching_signals: vec![AccountMatchSignal {
            kind: MatchSignalKind::ExactHandleReuse,
            description: "Exact handle `synth_unit_ops` appears on both synthetic platforms."
                .to_owned(),
            citation: SourceCitation {
                source_id: OpaqueId::new("source:directory")?,
                source_version: src_v1.clone(),
                anchor: ExposureCitationAnchor::StructuredRecordField {
                    record_locator: "entry/99".to_owned(),
                    field_name: "handle".to_owned(),
                },
            },
        }],
        contradicting_signals: vec![
            "Directory entry allows unverified user submissions.".to_owned(),
        ],
        ambiguity_explanation: "Handle is generic; unverified directory listing could be a spoof."
            .to_owned(),
        proposed_at: utc_instant(12_400),
    })?;

    assert_eq!(
        proposed.assessment_state(),
        MatchAssessmentState::ProposedCandidate
    );
    assert!(!proposed.assessment_state().is_user_confirmed());
    assert!(!proposed.assessment_state().establishes_automated_identity());
    assert_eq!(
        proposed.uncertainty_notice(),
        ACCOUNT_MATCH_UNCERTAINTY_NOTICE
    );

    // First review: mark as AmbiguousRetained.
    audit.review_account_match(AccountMatchReviewInput {
        match_id: match_id.clone(),
        expected_version: VersionId::new("ver:m1")?,
        new_version: VersionId::new("ver:m2")?,
        reviewer: auditor.clone(),
        reviewed_at: utc_instant(12_500),
        new_state: MatchAssessmentState::AmbiguousRetained,
        private_rationale_note:
            "PRIVATE-REVIEW-NOTE-1: Hold until domain admin confirms directory account.".to_owned(),
    })?;

    // Stale expected version must fail without mutating history.
    assert_eq!(
        audit.review_account_match(AccountMatchReviewInput {
            match_id: match_id.clone(),
            expected_version: VersionId::new("ver:m1")?,
            new_version: VersionId::new("ver:m3-stale")?,
            reviewer: auditor.clone(),
            reviewed_at: utc_instant(12_600),
            new_state: MatchAssessmentState::UserRejectedDifferentEntity,
            private_rationale_note: "Stale attempt.".to_owned(),
        }),
        Err(ExposureAuditError::StaleExpectedVersion {
            expected: VersionId::new("ver:m1")?,
            actual: VersionId::new("ver:m2")?,
        })
    );

    // Second review: transition to UserRejectedDifferentEntity; candidate and its signals remain inspectable.
    let rejected = audit.review_account_match(AccountMatchReviewInput {
        match_id: match_id.clone(),
        expected_version: VersionId::new("ver:m2")?,
        new_version: VersionId::new("ver:m3")?,
        reviewer: auditor,
        reviewed_at: utc_instant(12_700),
        new_state: MatchAssessmentState::UserRejectedDifferentEntity,
        private_rationale_note:
            "PRIVATE-REVIEW-NOTE-2: Confirmed third-party mirror, not owned by team.".to_owned(),
    })?;
    assert_eq!(
        rejected.assessment_state(),
        MatchAssessmentState::UserRejectedDifferentEntity
    );
    assert_eq!(rejected.matching_signals().len(), 1);
    assert_eq!(rejected.contradicting_signals().len(), 1);
    assert_eq!(rejected.review_history().len(), 2);
    let debug_history = format!("{:?}", rejected.review_history());
    assert!(!debug_history.contains("PRIVATE-REVIEW-NOTE-1"));
    assert!(!debug_history.contains("PRIVATE-REVIEW-NOTE-2"));
    assert!(debug_history.contains("<redacted-local-note>"));

    audit.record_infrastructure_entry(InfrastructureEntryInput {
        entry_id: OpaqueId::new("infra:contact-1")?,
        version_id: VersionId::new("ver:infra-1")?,
        origin: AccountOrigin::Simulated,
        domain: "example.com".to_owned(),
        artifact: InfrastructureArtifactKind::ExposedContact {
            contact_kind: ExposedContactKind::AdministrativeContact,
            exposed_value: "contact@example.com".to_owned(),
            context_location: "Synthetic Directory WHOIS mirror".to_owned(),
            remediation_note: "Enable registrar WHOIS privacy redaction.".to_owned(),
        },
        citation: SourceCitation {
            source_id: OpaqueId::new("source:directory")?,
            source_version: src_v1.clone(),
            anchor: ExposureCitationAnchor::StructuredRecordField {
                record_locator: "entry/99".to_owned(),
                field_name: "whois".to_owned(),
            },
        },
        timestamps: ObservationTimeEnvelope {
            observed_or_valid_from: Some(utc_instant(10_000)),
            published_or_issued_at: None,
            retrieved_at: utc_instant(12_200),
            audited_at: utc_instant(12_350),
        },
    })?;

    audit.record_source_coverage(SourceCoverageRecord {
        coverage_id: OpaqueId::new("cov:dir-1")?,
        source_id: OpaqueId::new("source:directory")?,
        source_version: src_v1.clone(),
        queried_target: "example.com".to_owned(),
        queried_at: utc_instant(12_200),
        coverage: SourceQueryCoverage::ObservedAbsenceInQueriedSource {
            scope_limitation_note:
                "Zero historic TXT records in this mirror; does not prove absence across all DNS history."
                    .to_owned(),
        },
    })?;

    // Superseding src_v1 flags the profile, the retained rejected candidate match, and the infrastructure entry.
    let flagged = audit.flag_source_version_updated(
        &src_v1,
        &src_v2,
        "Directory corrected stale contact parsing on entry/99.",
    )?;
    assert_eq!(flagged, 3);
    assert!(audit.account_matches().contains_key(&match_id));

    let summary = audit.summary();
    assert_eq!(summary.items_needing_reassessment, 3);
    assert_eq!(summary.user_rejected_match_count, 1);
    assert_eq!(summary.exposed_contact_entry_count, 1);

    Ok(())
}
