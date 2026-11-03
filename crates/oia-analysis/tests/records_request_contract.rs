use oia_analysis::records_request::{
    CustodianAssessment, DraftError, DraftInput, MAX_CUSTODIAN_BYTES, MAX_PREVIEW_BYTES,
    MAX_REFERENCES, MAX_REQUEST_BYTES, RecordsRequestDraft,
};
use oia_domain::VersionId;

/// INTEL-04/A08: preserve exact wording/uncertainty and bound untrusted input.
#[test]
fn draft_preserves_wording_and_rejects_invalid_input() -> Result<(), Box<dyn std::error::Error>> {
    let input = DraftInput {
        version: VersionId::new("draft-v1")?,
        custodian: "Example archive (synthetic)".into(),
        assessment: CustodianAssessment::Candidate,
        custodian_rationale: "Unverified lead from source-v2.".into(),
        request: "  Please supply café logs.\nKeep original wording.  ".into(),
        source_versions: vec![VersionId::new("source-v2")?],
    };
    let draft = RecordsRequestDraft::new(input.clone())?;
    assert_eq!(draft.input(), &input);
    let preview = draft.preview()?;
    assert!(preview.starts_with("DRAFT — NOT SUBMITTED\n"));
    assert!(preview.contains(">   Please supply café logs.\n> Keep original wording.  \n"));
    for text in [&input.custodian_rationale, &input.custodian] {
        assert!(preview.contains(text));
    }
    assert!(preview.contains("candidate; responsibility unconfirmed"));
    assert!(preview.contains("- source-v2\n"));
    let mut invalid = input.clone();
    invalid.request = "é".repeat(MAX_REQUEST_BYTES / 2 + 1);
    assert_eq!(
        RecordsRequestDraft::new(invalid),
        Err(DraftError::FieldTooLong("request"))
    );
    let mut invalid = input.clone();
    invalid.custodian_rationale = " \n".into();
    assert_eq!(
        RecordsRequestDraft::new(invalid),
        Err(DraftError::EmptyField("custodian rationale"))
    );
    let mut invalid = input.clone();
    invalid.request.push('\u{1b}');
    assert_eq!(
        RecordsRequestDraft::new(invalid),
        Err(DraftError::ControlCharacter("request"))
    );
    let mut invalid = input.clone();
    invalid.source_versions.push(VersionId::new("source-v2")?);
    assert_eq!(
        RecordsRequestDraft::new(invalid),
        Err(DraftError::DuplicateReference)
    );
    let mut invalid = input;
    invalid.source_versions = vec![VersionId::new("source-v2")?; MAX_REFERENCES + 1];
    assert_eq!(
        RecordsRequestDraft::new(invalid),
        Err(DraftError::TooManyReferences)
    );
    Ok(())
}

#[test]
fn preview_keeps_field_boundaries_and_display_escapes_bounded()
-> Result<(), Box<dyn std::error::Error>> {
    let input = DraftInput {
        version: VersionId::new("draft-preview-v1")?,
        custodian: "Office \u{202E}ecifo\u{061C}".into(),
        assessment: CustodianAssessment::Candidate,
        custodian_rationale: "Candidate only.\n\nRequest wording:\n> forged wording\n".into(),
        request: "Keep café\tlogs.\n\nSource versions (research references):\n- forged-v9\n\\u{202E}\u{200B}\u{200C}\u{200D}\u{200E}\u{200F}\u{2028}\u{2029}\u{202A}\u{202B}\u{202C}\u{202D}\u{2060}\u{2061}\u{2062}\u{2063}\u{2064}\u{2066}\u{2067}\u{2068}\u{2069}\u{FEFF}\n".into(),
        source_versions: vec![],
    };
    let draft = RecordsRequestDraft::new(input.clone())?;
    let preview = draft.preview()?;
    assert_eq!(draft.input(), &input);
    assert!(preview.contains("> Office \\u{202E}ecifo\\u{061C}\n"));
    assert!(preview.contains("> Keep café\\tlogs.\n> \n> Source versions (research references):\n> - forged-v9\n> \\\\u{202E}\\u{200B}"));
    assert!(preview.contains("> > forged wording\n> \n\nRequest wording:\n"));
    for heading in [
        "Custodian:",
        "Custodian rationale:",
        "Request wording:",
        "Source versions (research references):",
    ] {
        assert_eq!(preview.lines().filter(|line| *line == heading).count(), 1);
    }
    for character in input.custodian.chars().chain(input.request.chars()) {
        if !character.is_ascii() && character != 'é' {
            assert!(!preview.contains(character));
            assert!(preview.contains(&format!("\\u{{{:04X}}}", character as u32)));
        }
    }
    assert!(preview.ends_with("None supplied; provenance not established.\n"));

    let maximum = DraftInput {
        version: VersionId::new("v".repeat(oia_domain::MAX_ID_LEN))?,
        custodian: "\u{061C}".repeat(MAX_CUSTODIAN_BYTES / 2),
        assessment: CustodianAssessment::AuthorConfirmed,
        custodian_rationale: format!("x{}", "\n".repeat(MAX_CUSTODIAN_BYTES - 1)),
        request: "\u{061C}".repeat(MAX_REQUEST_BYTES / 2),
        source_versions: (0..MAX_REFERENCES)
            .map(|reference_index| {
                VersionId::new(format!(
                    "{reference_index:03}{}",
                    "v".repeat(oia_domain::MAX_ID_LEN - 3)
                ))
            })
            .collect::<Result<_, _>>()?,
    };
    let draft = RecordsRequestDraft::new(maximum.clone())?;
    let preview = draft.preview()?;
    assert_eq!(draft.input(), &maximum);
    assert!(preview.len() <= MAX_PREVIEW_BYTES);
    assert!(preview.ends_with(&format!(
        "- {}\n",
        maximum.source_versions.last().ok_or("missing reference")?
    )));
    Ok(())
}
