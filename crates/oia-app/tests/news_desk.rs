use oia_app::local_tools::Control;
use oia_app::news::{Dossier, EXAMPLE as WEB_SNAPSHOT, Filter, MAX_BYTES, ReportKind};
use sha2::{Digest, Sha256};

const EXAMPLE: &[u8] = include_bytes!("fixtures/synthetic-news-dossier.json");

#[test]
fn real_snapshot_retains_provenance_without_inventing_lifecycle()
-> Result<(), Box<dyn std::error::Error>> {
    let control = Control::default();
    let dossier = Dossier::parse(WEB_SNAPSHOT, &control)?;
    let view = dossier.view(&Filter::default(), &control)?;
    assert!(!view.synthetic);
    assert!(matches!(
        view.collection_coverage,
        oia_app::news::CollectionCoverage::Partial
    ));
    assert_eq!((view.total.versions, view.total.sources), (3, 1));
    assert_eq!(dossier.original_bytes(), WEB_SNAPSHOT);
    for entry in &view.articles {
        let article = &entry.article;
        assert!(matches!(
            article.retraction_status,
            oia_app::news::RetractionStatus::Unknown
        ));
        assert!(article.event_time.is_none() && article.origin_id.is_none());
        assert!(article.event_id.is_none() && article.predecessor_version_id.is_none());
        let provenance = article.provenance.as_ref().ok_or("Missing provenance")?;
        assert!(provenance.original_excerpt_html.starts_with("<p "));
        assert!(provenance.retrieved_time.starts_with("2026-11-05T"));
        assert_eq!(article.published_time.as_ref().map(String::len), Some(10));
        let text = article.text.as_ref().ok_or("Missing excerpt")?;
        assert_eq!(
            entry.text_sha256,
            Some(format!("{:x}", Sha256::digest(text)))
        );
    }
    let mut oversized: serde_json::Value = serde_json::from_slice(WEB_SNAPSHOT)?;
    oversized["articles"][0]["provenance"]["original_excerpt_html"] =
        serde_json::json!("x".repeat(64 * 1024 + 1));
    assert_eq!(
        Dossier::parse(&serde_json::to_vec(&oversized)?, &control)
            .err()
            .map(|error| error.code),
        Some("invalid_news")
    );
    Ok(())
}

#[test]
fn versions_and_unknown_origins_survive_filtered_views() -> Result<(), Box<dyn std::error::Error>> {
    let control = Control::default();
    let dossier = Dossier::parse(EXAMPLE, &control)?;
    let view = dossier.view(&Filter::default(), &control)?;
    assert_eq!(dossier.original_bytes(), EXAMPLE);
    assert_eq!(dossier.sha256(), format!("{:x}", Sha256::digest(EXAMPLE)));
    assert!(view.synthetic);
    assert_eq!(
        (view.total.versions, view.total.records, view.total.sources),
        (5, 4, 4)
    );
    assert_eq!(view.total.unknown_origin_versions, 1);
    assert_eq!(view.articles[2].lineage[0].to_version_id, "wire-account@v1");
    assert!(matches!(
        view.articles[1].article.retraction_status,
        oia_app::news::RetractionStatus::CorrectedByLaterVersion
    ));
    assert_eq!(
        view.articles[3].article.predecessor_version_id.as_deref(),
        Some("wire-account@v1")
    );
    assert_eq!(
        (
            view.total.proposed_events,
            view.total.ungrouped_versions,
            view.total.missing_text_versions
        ),
        (1, 1, 1)
    );
    assert_eq!(view.articles[1].text_sha256, view.articles[2].text_sha256);
    assert_ne!(
        view.articles[1].article.record_id,
        view.articles[2].article.record_id
    );
    assert_ne!(view.articles[1].article.text, view.articles[3].article.text);
    assert!(matches!(
        view.articles[4].article.availability,
        oia_app::news::AvailabilityState::Missing
    ));
    assert!(matches!(
        view.articles[4].article.retraction_status,
        oia_app::news::RetractionStatus::NotRetracted
    ));
    assert!(
        serde_json::to_value(&view)?
            .get("total")
            .and_then(|counts| counts.get("declared_origins"))
            .is_none()
    );
    let filtered = dossier.view(
        &Filter {
            query: "fourteen".to_string(),
            region: Some("Vesper Harbor (fictional)".to_string()),
            kind: Some(ReportKind::Allegation),
        },
        &control,
    )?;
    assert_eq!(
        (
            filtered.visible.versions,
            filtered.visible.sources,
            filtered.visible.unknown_origin_versions
        ),
        (2, 2, 0)
    );
    assert_eq!(filtered.total.versions, 5);
    assert_eq!(filtered.source_roster.len(), 2);
    assert_eq!(
        dossier
            .view(
                &Filter {
                    query: "NO MATCH".to_string(),
                    ..Filter::default()
                },
                &control
            )?
            .visible
            .versions,
        0
    );
    assert_eq!(
        dossier.view(&Filter::default(), &control)?.articles.len(),
        5
    );
    assert_eq!(dossier.original_bytes(), EXAMPLE);
    Ok(())
}

#[test]
fn malformed_oversized_and_cancelled_imports_are_refused() -> Result<(), Box<dyn std::error::Error>>
{
    let control = Control::default();
    let input: serde_json::Value = serde_json::from_slice(EXAMPLE)?;
    let mut duplicate = input.clone();
    duplicate["articles"][1]["version_id"] = duplicate["articles"][0]["version_id"].clone();
    let mut inconsistent = input.clone();
    inconsistent["articles"][3]["source_id"] = serde_json::json!("another-source");
    let mut oversized = input.clone();
    oversized["articles"][0]["text"] = serde_json::json!("x".repeat(64 * 1024 + 1));
    let mut invalid_kind = input.clone();
    invalid_kind["articles"][0]["kind"] = serde_json::json!("confirmed_truth");
    let mut dangling = input.clone();
    dangling["lineage"][0]["to_version_id"] = serde_json::json!("absent@v1");
    let mut broken_correction = input.clone();
    broken_correction["articles"][3]["predecessor_version_id"] = serde_json::Value::Null;
    let mut cycle = input.clone();
    cycle["articles"][1]["predecessor_version_id"] = serde_json::json!("wire-account@v2");
    cycle["articles"][3]["superseded_by_version_id"] = serde_json::json!("wire-account@v1");
    let mut removed_retracted = input.clone();
    removed_retracted["articles"][4]["availability"] = serde_json::json!("removed");
    removed_retracted["articles"][4]["retraction_status"] = serde_json::json!("retracted");
    let mut unknown_field = input;
    unknown_field["permission_to_execute"] = serde_json::json!(true);
    for invalid in [
        duplicate,
        inconsistent,
        oversized,
        invalid_kind,
        unknown_field,
        dangling,
        broken_correction,
        cycle,
        removed_retracted,
    ] {
        assert_eq!(
            Dossier::parse(&serde_json::to_vec(&invalid)?, &control)
                .err()
                .map(|error| error.code),
            Some("invalid_news")
        );
    }
    assert!(Dossier::parse(&vec![b' '; MAX_BYTES + 1], &control).is_err());
    let dossier = Dossier::parse(EXAMPLE, &control)?;
    assert!(
        dossier
            .view(
                &Filter {
                    query: "é".repeat(129),
                    ..Filter::default()
                },
                &control
            )
            .is_err()
    );
    control.cancel();
    assert_eq!(
        Dossier::parse(EXAMPLE, &control)
            .err()
            .map(|error| error.code),
        Some("cancelled")
    );
    assert_eq!(
        dossier
            .view(&Filter::default(), &control)
            .err()
            .map(|error| error.code),
        Some("cancelled")
    );
    Ok(())
}
