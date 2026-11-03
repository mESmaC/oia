//! Targeted contract tests for the personal public exposure and owned-infrastructure
//! audit kernel (`#47`, `PRIVACY-02`, `A18`).
//!
//! Uses only synthetic personas, reserved domains (`example.com`, `example.org`, `example.net`,
//! `.invalid`, `.test`), and documentation IP ranges (`192.0.2.0/24`, `2001:db8::/32`).

use oia_analysis::exposure_audit::{
    ACCOUNT_MATCH_UNCERTAINTY_NOTICE, AccountMatchInput, AccountMatchReviewInput,
    AccountMatchSignal, AuditLimitKind, AuditTimePrecision, AuditTimestamp, DeclaredAccountHandle,
    DeclaredAuditScope, DnsRecordType, ExposedContactKind, ExposureAuditError, ExposureAuditLimits,
    ExposureCitationAnchor, InfrastructureArtifactKind, InfrastructureEntryInput,
    MatchAssessmentState, MatchSignalKind, ObservationTimeEnvelope, OutboundQueryTarget,
    OutboundSourceQueryCandidate, OwnershipBasis, PersonalExposureAudit, PublicProfileInput,
    ScopeRevisionInput, SourceCitation, SourceCoverageRecord, SourceQueryCoverage,
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

/// Protects `PRIVACY-02` and `A18` scope boundaries, service-qualified handle claiming,
/// third-party listed-domain non-pivoting, dynamic scope re-checking across narrowings,
/// explicit scope revision, and collaboration access-policy integration:
/// - `DeclaredAuditScope` requires at least one valid domain or `(service_name, handle)` root,
///   rejects IPv4/IPv6 literals and bare public suffixes (`co.uk`), and normalizes case/trailing dots.
/// - Subdomain matching enforces a strict dot boundary (`telemetry.example.org` is covered by `example.org`,
///   whereas lookalike `myexample.org` is rejected with `DomainOutsideDeclaredScope`).
/// - A profile is accepted with `claimed_in_declared_scope: true` **only** when its `(service_name, handle)`
///   pair matches a declared pair (`covers_handle`). A third-party profile (`mallory_fict`) that merely lists
///   `example.org` in `listed_website_domain` is refused as claimed (`HandleOutsideDeclaredScope`), and the
///   same handle on a different service is also refused as claimed.
/// - When `mallory_fict` is registered as an unclaimed candidate (`claimed_in_declared_scope: false`, tied
///   to an in-scope `anchor_primary_profile_id`), its link to `example.org` can be recorded as a
///   `CrossLinkToDeclaredDomain` signal from the user's primary profile, but `mallory_fict` cannot be used
///   as `primary_profile_id` to pivot to another third party (`PrimaryProfileMustBeInDeclaredScope`).
/// - Narrowing `DeclaredAuditScope` via `revise_declared_scope` re-evaluates existing records, marking
///   dropped profiles, matches, and infrastructure entries with `outside_current_scope() == true` and
///   `needs_reassessment() == true` while retaining their history, and subsequent `propose_account_match`
///   or `review_account_match` calls using a narrowed-out primary profile are refused.
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

    // Reject IPv4 literals and bare multi-label public suffixes as declared root domains.
    assert_eq!(
        DeclaredAuditScope::new(
            OpaqueId::new("scope:ipv4")?,
            VersionId::new("ver:s-ipv4")?,
            1_000,
            OwnershipBasis::SelfOwnedPersonalScope {
                owner_actor: owner.clone(),
                verification_note: "IPv4 literal check.".to_owned(),
            },
            vec!["192.0.2.1".to_owned()],
            Vec::new(),
            limits,
        ),
        Err(ExposureAuditError::InvalidDomainSyntax)
    );
    assert_eq!(
        DeclaredAuditScope::new(
            OpaqueId::new("scope:psl")?,
            VersionId::new("ver:s-psl")?,
            1_000,
            OwnershipBasis::SelfOwnedPersonalScope {
                owner_actor: owner.clone(),
                verification_note: "Public suffix check.".to_owned(),
            },
            vec!["co.uk".to_owned()],
            Vec::new(),
            limits,
        ),
        Err(ExposureAuditError::PublicSuffixRootDisallowed {
            domain: "co.uk".to_owned(),
        })
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
        vec![DeclaredAccountHandle::new(
            "SyntheticGit",
            "Fictional_Handle_1",
        )],
        limits,
    )?;

    assert!(scope.covers_domain("example.org"));
    assert!(scope.covers_domain("telemetry.example.org"));
    assert!(!scope.covers_domain("myexample.org"));
    assert!(!scope.covers_domain("example.org.evil.invalid"));
    assert!(scope.covers_handle("syntheticgit", "fictional_handle_1"));
    assert!(!scope.covers_handle("otherservice", "fictional_handle_1"));

    let debug_scope = format!("{scope:?}");
    assert!(!debug_scope.contains("SECRET-LOCAL-OWNERSHIP-NOTE"));
    assert!(debug_scope.contains("<redacted-local-note>"));

    // PersonalExposureAudit::new revalidates scope against the audit's own limits.
    let tight_scope_limits = ExposureAuditLimits::new(512, 1, 4, 8, 8, 4, 8, 8)?;
    assert_eq!(
        PersonalExposureAudit::new(
            OpaqueId::new("audit:too-many-targets")?,
            SchemaVersion::V1,
            scope.clone(),
            tight_scope_limits,
        ),
        Err(ExposureAuditError::LimitExceeded {
            limit: AuditLimitKind::ScopeTargets,
            actual: 2,
            max: 1,
        })
    );

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

    // Register the in-scope primary profile `(SyntheticGit, fictional_handle_1)`.
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
        anchor_primary_profile_id: None,
        citation: SourceCitation {
            source_id: OpaqueId::new("source:git")?,
            source_version: VersionId::new("ver:git-1")?,
            anchor: ExposureCitationAnchor::Utf8ByteRange(0..32),
        },
        timestamps: ObservationTimeEnvelope {
            observed_at: Some(utc_instant(1_500)),
            published_or_issued_at: None,
            retrieved_at: utc_instant(1_600),
            audited_at: utc_instant(1_700),
        },
    })?;

    // B1 Repro A: A third-party profile (`mallory_fict`) that lists `example.org` as its website
    // is NOT accepted as `claimed_in_declared_scope: true`.
    assert_eq!(
        audit.register_public_profile(PublicProfileInput {
            profile_id: OpaqueId::new("prof:mallory-claimed")?,
            version_id: VersionId::new("ver:prof-mallory-claimed")?,
            origin: AccountOrigin::Simulated,
            service_name: "SyntheticGit".to_owned(),
            handle: "mallory_fict".to_owned(),
            display_name: None,
            bio_excerpt: None,
            listed_website_domain: Some("example.org".to_owned()),
            exposed_contact_values: Vec::new(),
            claimed_in_declared_scope: true,
            anchor_primary_profile_id: None,
            citation: SourceCitation {
                source_id: OpaqueId::new("source:git")?,
                source_version: VersionId::new("ver:git-1")?,
                anchor: ExposureCitationAnchor::Utf8ByteRange(0..32),
            },
            timestamps: ObservationTimeEnvelope {
                observed_at: None,
                published_or_issued_at: None,
                retrieved_at: utc_instant(1_610),
                audited_at: utc_instant(1_710),
            },
        }),
        Err(ExposureAuditError::HandleOutsideDeclaredScope)
    );

    // B1 Repro B: The same handle (`fictional_handle_1`) on a different service (`OtherService`)
    // is NOT accepted as `claimed_in_declared_scope: true`.
    assert_eq!(
        audit.register_public_profile(PublicProfileInput {
            profile_id: OpaqueId::new("prof:other-service-claimed")?,
            version_id: VersionId::new("ver:prof-other-service")?,
            origin: AccountOrigin::Simulated,
            service_name: "OtherService".to_owned(),
            handle: "fictional_handle_1".to_owned(),
            display_name: None,
            bio_excerpt: None,
            listed_website_domain: None,
            exposed_contact_values: Vec::new(),
            claimed_in_declared_scope: true,
            anchor_primary_profile_id: None,
            citation: SourceCitation {
                source_id: OpaqueId::new("source:other")?,
                source_version: VersionId::new("ver:other-1")?,
                anchor: ExposureCitationAnchor::Utf8ByteRange(0..32),
            },
            timestamps: ObservationTimeEnvelope {
                observed_at: None,
                published_or_issued_at: None,
                retrieved_at: utc_instant(1_620),
                audited_at: utc_instant(1_720),
            },
        }),
        Err(ExposureAuditError::HandleOutsideDeclaredScope)
    );

    // Unclaimed third-party profiles without an in-scope `anchor_primary_profile_id` are refused.
    assert_eq!(
        audit.register_public_profile(PublicProfileInput {
            profile_id: OpaqueId::new("prof:unanchored")?,
            version_id: VersionId::new("ver:prof-unanchored")?,
            origin: AccountOrigin::Simulated,
            service_name: "SyntheticForum".to_owned(),
            handle: "unrelated_third_party".to_owned(),
            display_name: None,
            bio_excerpt: None,
            listed_website_domain: None,
            exposed_contact_values: Vec::new(),
            claimed_in_declared_scope: false,
            anchor_primary_profile_id: None,
            citation: SourceCitation {
                source_id: OpaqueId::new("source:forum")?,
                source_version: VersionId::new("ver:forum-1")?,
                anchor: ExposureCitationAnchor::Utf8ByteRange(0..32),
            },
            timestamps: ObservationTimeEnvelope {
                observed_at: None,
                published_or_issued_at: None,
                retrieved_at: utc_instant(1_630),
                audited_at: utc_instant(1_730),
            },
        }),
        Err(ExposureAuditError::UnclaimedProfileRequiresInScopePrimaryAnchor)
    );

    // Register `mallory_fict` (listing `example.org`) and `fictional_handle_1_alt` as unclaimed candidates
    // anchored to `prof:in-scope`.
    audit.register_public_profile(PublicProfileInput {
        profile_id: OpaqueId::new("prof:mallory-candidate")?,
        version_id: VersionId::new("ver:prof-mallory-cand")?,
        origin: AccountOrigin::Simulated,
        service_name: "SyntheticForum".to_owned(),
        handle: "mallory_fict".to_owned(),
        display_name: None,
        bio_excerpt: None,
        listed_website_domain: Some("example.org".to_owned()),
        exposed_contact_values: Vec::new(),
        claimed_in_declared_scope: false,
        anchor_primary_profile_id: Some(OpaqueId::new("prof:in-scope")?),
        citation: SourceCitation {
            source_id: OpaqueId::new("source:forum")?,
            source_version: VersionId::new("ver:forum-1")?,
            anchor: ExposureCitationAnchor::Utf8ByteRange(0..48),
        },
        timestamps: ObservationTimeEnvelope {
            observed_at: None,
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
        anchor_primary_profile_id: Some(OpaqueId::new("prof:in-scope")?),
        citation: SourceCitation {
            source_id: OpaqueId::new("source:other")?,
            source_version: VersionId::new("ver:other-1")?,
            anchor: ExposureCitationAnchor::Utf8ByteRange(0..48),
        },
        timestamps: ObservationTimeEnvelope {
            observed_at: None,
            published_or_issued_at: None,
            retrieved_at: utc_instant(1_660),
            audited_at: utc_instant(1_760),
        },
    })?;

    // A profile admitted as an unclaimed candidate listing `example.org` (`prof:mallory-candidate`)
    // can be the *target* of a `CrossLinkToDeclaredDomain` candidate match from `prof:in-scope`,
    // but CANNOT be used as `primary_profile_id` to pivot to another profile.
    assert_eq!(
        audit.propose_account_match(AccountMatchInput {
            match_id: OpaqueId::new("match:mallory-pivot")?,
            version_id: VersionId::new("ver:match-mallory-pivot")?,
            primary_profile_id: OpaqueId::new("prof:mallory-candidate")?,
            candidate_profile_id: OpaqueId::new("prof:third-party-b")?,
            uncalibrated_heuristic_score: 0.75,
            matching_signals: vec![AccountMatchSignal {
                kind: MatchSignalKind::CrossLinkToDeclaredDomain,
                description: "Attempted pivot from profile that only listed example.org."
                    .to_owned(),
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
            profile_id: OpaqueId::new("prof:mallory-candidate")?,
        })
    );

    // Refusal: Out-of-scope handle or wrong service on outbound source queries is refused.
    assert_eq!(
        audit.prepare_read_only_source_query(OutboundSourceQueryCandidate {
            source_id: OpaqueId::new("source:forum")?,
            target: OutboundQueryTarget::DeclaredHandle {
                service_name: "SyntheticForum".to_owned(),
                handle: "fictional_handle_1".to_owned(),
            },
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

    // Reject out-of-scope certificate subject_common_name (even with empty SANs) and out-of-scope SAN.
    assert_eq!(
        audit.record_infrastructure_entry(InfrastructureEntryInput {
            entry_id: OpaqueId::new("infra:cert-bad-cn")?,
            version_id: VersionId::new("ver:cert-bad-cn-1")?,
            origin: AccountOrigin::Simulated,
            domain: "example.org".to_owned(),
            artifact: InfrastructureArtifactKind::CertificateHistory {
                certificate_sha256_fingerprint: "a".repeat(64),
                serial_number: "01:02".to_owned(),
                issuer_common_name: "Synthetic Test CA".to_owned(),
                subject_common_name: "unrelated.example.net".to_owned(),
                subject_alt_names: Vec::new(),
                not_before_unix_ms: 1_000,
                not_after_unix_ms: 5_000,
            },
            citation: SourceCitation {
                source_id: OpaqueId::new("source:ct")?,
                source_version: VersionId::new("ver:ct-1")?,
                anchor: ExposureCitationAnchor::Utf8ByteRange(0..20),
            },
            timestamps: ObservationTimeEnvelope {
                observed_at: Some(utc_instant(1_000)),
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
            owner_actor: owner.clone(),
            verification_note: "Verified secondary domain example.net via DNS TXT.".to_owned(),
        },
        declared_root_domains: vec!["example.org".to_owned(), "example.net".to_owned()],
        declared_handles: vec![DeclaredAccountHandle::new(
            "SyntheticGit",
            "fictional_handle_1",
        )],
        revision_rationale:
            "Added secondary personal domain example.net after confirming ownership.".to_owned(),
    })?;
    assert_eq!(
        revised_scope.predecessor_scope_version(),
        Some(&VersionId::new("ver:scope-1")?)
    );
    assert_eq!(audit.prior_scopes().len(), 1);

    // Record an infrastructure entry on `example.net` while it is in scope.
    audit.record_infrastructure_entry(InfrastructureEntryInput {
        entry_id: OpaqueId::new("infra:net-dns")?,
        version_id: VersionId::new("ver:net-dns-1")?,
        origin: AccountOrigin::Simulated,
        domain: "api.example.net".to_owned(),
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
            observed_at: Some(utc_instant(2_510)),
            published_or_issued_at: None,
            retrieved_at: utc_instant(2_520),
            audited_at: utc_instant(2_530),
        },
    })?;

    // B2 check: Narrow the scope to drop `(SyntheticGit, fictional_handle_1)` and `example.net`.
    // Existing records must be retained, marked `outside_current_scope == true` and `needs_reassessment == true`,
    // and `prof:in-scope` must now be refused as a match primary!
    audit.revise_declared_scope(ScopeRevisionInput {
        expected_scope_version: VersionId::new("ver:scope-2")?,
        new_scope_version: VersionId::new("ver:scope-3")?,
        revised_at_unix_ms: 2_700,
        ownership_basis: OwnershipBasis::SelfOwnedPersonalScope {
            owner_actor: owner,
            verification_note: "Narrowed scope to example.org only.".to_owned(),
        },
        declared_root_domains: vec!["example.org".to_owned()],
        declared_handles: Vec::new(),
        revision_rationale: "Retired syntheticgit handle and example.net from active scope."
            .to_owned(),
    })?;

    let narrowed_profile = audit
        .profiles()
        .get(&OpaqueId::new("prof:in-scope")?)
        .ok_or("missing prof:in-scope")?;
    assert!(narrowed_profile.outside_current_scope());
    assert!(narrowed_profile.needs_reassessment());

    let narrowed_infra = audit
        .infrastructure_entries()
        .get(&OpaqueId::new("infra:net-dns")?)
        .ok_or("missing infra:net-dns")?;
    assert!(narrowed_infra.outside_current_scope());
    assert!(narrowed_infra.needs_reassessment());

    // Proposing a match with `prof:in-scope` after narrowing is now refused (`PrimaryProfileMustBeInDeclaredScope`).
    assert_eq!(
        audit.propose_account_match(AccountMatchInput {
            match_id: OpaqueId::new("match:after-narrowing")?,
            version_id: VersionId::new("ver:match-after-narrowing")?,
            primary_profile_id: OpaqueId::new("prof:in-scope")?,
            candidate_profile_id: OpaqueId::new("prof:mallory-candidate")?,
            uncalibrated_heuristic_score: 0.50,
            matching_signals: vec![AccountMatchSignal {
                kind: MatchSignalKind::CrossLinkToDeclaredDomain,
                description: "Listed example.org.".to_owned(),
                citation: SourceCitation {
                    source_id: OpaqueId::new("source:forum")?,
                    source_version: VersionId::new("ver:forum-1")?,
                    anchor: ExposureCitationAnchor::Utf8ByteRange(0..16),
                },
            }],
            contradicting_signals: Vec::new(),
            ambiguity_explanation: "Primary handle was removed from scope.".to_owned(),
            proposed_at: utc_instant(2_800),
        }),
        Err(ExposureAuditError::PrimaryProfileMustBeInDeclaredScope {
            profile_id: OpaqueId::new("prof:in-scope")?,
        })
    );

    // Block outbound source queries that would leak private local analysis or full scope manifests.
    assert!(matches!(
        audit.prepare_read_only_source_query(OutboundSourceQueryCandidate {
            source_id: OpaqueId::new("source:ct")?,
            target: OutboundQueryTarget::DeclaredDomain("example.org".to_owned()),
            requested_at_unix_ms: 2_800,
            attaches_private_notes_or_hypotheses: true,
            exports_full_scope_manifest: false,
            mutates_remote_account_or_dns: false,
        }),
        Err(ExposureAuditError::PrivateAssessmentLeakBlocked(_))
    ));

    Ok(())
}

/// Protects `PRIVACY-02`, `A18`, and `AGENTS.md` §2 epistemic, authorization-window, actor-check,
/// and provenance rules:
/// - `AuthorizedAssessmentScope` records who authorized it, what they authorized, and `[valid_from_unix_ms, valid_until_unix_ms]`,
///   and rejects actions (including `revise_declared_scope`) outside that window (`AuthorizationExpiredOrNotYetValid`)
///   or by an unauthorized actor (`UnauthorizedActor`).
/// - Even a `1.0` `uncalibrated_heuristic_score` starts in `MatchAssessmentState::ProposedCandidate`
///   and `establishes_automated_identity()` is always `false`.
/// - Ambiguous account matches can be reviewed as `AmbiguousRetained`, and later rejected as
///   `UserRejectedDifferentEntity` while keeping the rejected candidate, its evidence signals, and its
///   append-only review history intact for later inspection.
/// - Updating/superseding an upstream `(source_id, source_version)` flags only dependent profiles,
///   account matches, and infrastructure entries citing that `source_id` and `source_version`
///   with `needs_reassessment = true` without erasing prior reviews.
#[test]
fn authorized_window_ambiguous_match_retention_and_source_reassessment()
-> Result<(), Box<dyn std::error::Error>> {
    let auditor = ActorId::new("actor:synthetic-auditor")?;
    let intruding_actor = ActorId::new("actor:intruder")?;
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
                "Authorized exposure review of example.com and handle synth_unit_ops on SyntheticCodeHost."
                    .to_owned(),
            valid_from_unix_ms: 10_000,
            valid_until_unix_ms: 20_000,
            verification_note: "Signed authorization fixture in local vault.".to_owned(),
        },
        vec!["example.com".to_owned()],
        vec![DeclaredAccountHandle::new(
            "SyntheticCodeHost",
            "synth_unit_ops",
        )],
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

    // Attempting to revise an expired `AuthorizedAssessmentScope` into a `SelfOwnedPersonalScope`
    // after `valid_until_unix_ms` is refused (`AuthorizationExpiredOrNotYetValid`).
    assert_eq!(
        audit.revise_declared_scope(ScopeRevisionInput {
            expected_scope_version: VersionId::new("ver:scope-v1")?,
            new_scope_version: VersionId::new("ver:scope-expired-escape")?,
            revised_at_unix_ms: 25_000,
            ownership_basis: OwnershipBasis::SelfOwnedPersonalScope {
                owner_actor: auditor.clone(),
                verification_note: "Attempted post-expiry switch.".to_owned(),
            },
            declared_root_domains: vec!["example.com".to_owned()],
            declared_handles: Vec::new(),
            revision_rationale: "Attempted post-expiry revision.".to_owned(),
        }),
        Err(ExposureAuditError::AuthorizationExpiredOrNotYetValid {
            action_unix_ms: 25_000,
            valid_from_unix_ms: 10_000,
            valid_until_unix_ms: 20_000,
        })
    );

    let src_dir_id = OpaqueId::new("source:directory")?;
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
        anchor_primary_profile_id: None,
        citation: SourceCitation {
            source_id: OpaqueId::new("source:codehost")?,
            source_version: VersionId::new("ver:src-codehost-v1")?,
            anchor: ExposureCitationAnchor::Utf8ByteRange(0..64),
        },
        timestamps: ObservationTimeEnvelope {
            observed_at: Some(utc_instant(11_000)),
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
        anchor_primary_profile_id: Some(OpaqueId::new("prof:declared-main")?),
        citation: SourceCitation {
            source_id: src_dir_id.clone(),
            source_version: src_v1.clone(),
            anchor: ExposureCitationAnchor::StructuredRecordField {
                record_locator: "entry/99".to_owned(),
                field_name: "profile".to_owned(),
            },
        },
        timestamps: ObservationTimeEnvelope {
            observed_at: None,
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
                source_id: src_dir_id.clone(),
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

    // An actor other than the declared auditor cannot record a review decision.
    assert_eq!(
        audit.review_account_match(AccountMatchReviewInput {
            match_id: match_id.clone(),
            expected_version: VersionId::new("ver:m1")?,
            new_version: VersionId::new("ver:m-intruder")?,
            reviewer: intruding_actor.clone(),
            reviewed_at: utc_instant(12_450),
            new_state: MatchAssessmentState::UserConfirmedSameOwner,
            private_rationale_note: "Unauthorized confirmation attempt.".to_owned(),
        }),
        Err(ExposureAuditError::UnauthorizedActor {
            expected: auditor.clone(),
            actual: intruding_actor,
        })
    );

    // Step 1 review: retain as ambiguous (`AmbiguousRetained`).
    audit.review_account_match(AccountMatchReviewInput {
        match_id: match_id.clone(),
        expected_version: VersionId::new("ver:m1")?,
        new_version: VersionId::new("ver:m2")?,
        reviewer: auditor.clone(),
        reviewed_at: utc_instant(12_500),
        new_state: MatchAssessmentState::AmbiguousRetained,
        private_rationale_note:
            "Holding as ambiguous until directory operator metadata is checked.".to_owned(),
    })?;

    // Step 2 review: user explicitly rejects the match as a different entity (`UserRejectedDifferentEntity`).
    // The candidate, its signals, and its review history MUST remain retained.
    let rejected = audit.review_account_match(AccountMatchReviewInput {
        match_id: match_id.clone(),
        expected_version: VersionId::new("ver:m2")?,
        new_version: VersionId::new("ver:m3")?,
        reviewer: auditor,
        reviewed_at: utc_instant(12_600),
        new_state: MatchAssessmentState::UserRejectedDifferentEntity,
        private_rationale_note:
            "Confirmed unrelated third-party mirror account; retaining rejection.".to_owned(),
    })?;

    assert_eq!(
        rejected.assessment_state(),
        MatchAssessmentState::UserRejectedDifferentEntity
    );
    assert_eq!(rejected.matching_signals().len(), 1);
    assert_eq!(rejected.contradicting_signals().len(), 1);
    assert_eq!(rejected.review_history().len(), 2);
    assert_eq!(
        rejected.review_history()[0].new_state(),
        MatchAssessmentState::AmbiguousRetained
    );
    assert_eq!(
        rejected.review_history()[1].new_state(),
        MatchAssessmentState::UserRejectedDifferentEntity
    );

    // Record infrastructure history and explicit source-query coverage.
    audit.record_infrastructure_entry(InfrastructureEntryInput {
        entry_id: OpaqueId::new("infra:contact-1")?,
        version_id: VersionId::new("ver:infra-1")?,
        origin: AccountOrigin::Simulated,
        domain: "sub.example.com".to_owned(),
        artifact: InfrastructureArtifactKind::ExposedContact {
            contact_kind: ExposedContactKind::EmailAddress,
            exposed_value: "admin@example.com".to_owned(),
            context_location: "Public directory WHOIS snapshot".to_owned(),
            remediation_note: "Enable registrar privacy redaction on example.com.".to_owned(),
        },
        citation: SourceCitation {
            source_id: src_dir_id.clone(),
            source_version: src_v1.clone(),
            anchor: ExposureCitationAnchor::ExtractedPassage {
                extraction_version: VersionId::new("ver:extract-1")?,
                passage_line: 4,
                byte_range: 10..42,
            },
        },
        timestamps: ObservationTimeEnvelope {
            observed_at: Some(utc_instant(11_500)),
            published_or_issued_at: Some(utc_instant(11_600)),
            retrieved_at: utc_instant(12_200),
            audited_at: utc_instant(12_700),
        },
    })?;

    audit.record_source_coverage(SourceCoverageRecord {
        coverage_id: OpaqueId::new("cov:1")?,
        source_id: src_dir_id.clone(),
        source_version: src_v1.clone(),
        queried_target: OutboundQueryTarget::DeclaredDomain("example.com".to_owned()),
        queried_at: utc_instant(12_200),
        coverage: SourceQueryCoverage::ObservedAbsenceInQueriedSource {
            scope_limitation_note: "No historical TXT records present in synthetic snapshot."
                .to_owned(),
        },
    })?;

    // Superseding a different source (`source:unrelated`) with the same version ID does NOT flag `source:directory` records.
    let unrelated_flagged = audit.flag_source_version_updated(
        &OpaqueId::new("source:unrelated")?,
        &src_v1,
        &src_v2,
        "Unrelated source update.",
    )?;
    assert_eq!(unrelated_flagged, 0);

    // Supersede `(source:directory, src_v1)` with `src_v2`: the profile, candidate match, and infrastructure entry citing it
    // must all be flagged `needs_reassessment == true` while preserving the rejected state and review trail.
    let flagged = audit.flag_source_version_updated(
        &src_dir_id,
        &src_v1,
        &src_v2,
        "Directory snapshot corrected upstream.",
    )?;
    assert_eq!(flagged, 3);

    let summary = audit.summary();
    assert_eq!(summary.user_rejected_match_count, 1);
    assert_eq!(summary.exposed_contact_entry_count, 1);
    assert_eq!(summary.items_outside_current_scope, 0);
    assert_eq!(summary.items_needing_reassessment, 3);

    let retained_match = audit
        .account_matches()
        .get(&match_id)
        .ok_or("missing match")?;
    assert!(retained_match.needs_reassessment());
    assert_eq!(
        retained_match.assessment_state(),
        MatchAssessmentState::UserRejectedDifferentEntity
    );
    assert_eq!(retained_match.review_history().len(), 2);

    Ok(())
}
