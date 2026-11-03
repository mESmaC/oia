use oia_app::local_tools::Control;
use oia_app::news::{Dossier, EXAMPLE, Filter, MAX_BYTES, ReportKind};
use sha2::{Digest, Sha256};

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
    assert_eq!(
        (
            view.total.declared_origins,
            view.total.unknown_origin_versions
        ),
        (2, 1)
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
        view.articles[4].article.publication_state,
        oia_app::news::PublicationState::Unavailable
    ));
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
            filtered.visible.declared_origins
        ),
        (2, 2, 1)
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
    let mut unknown_field = input;
    unknown_field["permission_to_execute"] = serde_json::json!(true);
    for invalid in [
        duplicate,
        inconsistent,
        oversized,
        invalid_kind,
        unknown_field,
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
