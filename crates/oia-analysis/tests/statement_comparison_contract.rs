//! Synthetic fixture texts stand in for reported accounts to establish the #138
//! kernel contract: copied wording lowers only the candidate-adjusted count, a
//! dismissal restores it, conflicting variants and omissions are preserved,
//! drift is reported per revision, simulated support is not counted as reported,
//! unsegmented text is not presented as a complete copy search, and the method
//! is printed. They are not measured performance on real accounts.
use oia_analysis::statement_comparison::{
    AspectStatus, CandidateDismissal, CodedValue, ComparisonError, ComparisonParameters,
    ComparisonRequest, CopySearch, DependencyReason, DetailAssertion, ExplanationExposure,
    Limitation, METHOD_ID, OverlapMeasure, Representative, SourceClass, StatementSource,
    StopReason, TextCapture, TextCoverage, compare_statements,
};
use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, VersionId};
use std::error::Error;
use std::ops::Range;

const NEWS: &str = "Residents reported a silent orange light hovering over the reservoir shortly \
after nine on Tuesday evening, according to the county sheriff's office.";
const COPIED: &str = "I saw it too. Residents reported a silent orange light hovering over the \
reservoir shortly after nine on Tuesday evening and I think it was a drone.";
const WITNESS_A1: &str = "Around 9:10 pm I was walking the dog by the reservoir. I saw an orange \
light, low over the water, completely silent. It hung there for about two minutes then moved north.";
const WITNESS_A2: &str = "Around 9:10 pm I was walking the dog by the reservoir. I saw a red \
light, low over the water. Maybe a faint hum. It hung there for about two minutes then moved north.";
const WITNESS_B: &str = "From my balcony on Hill Road I watched an orange glow above the \
reservoir. I could not tell how big it was. No sound that I noticed.";
const POLICE: &str = "Caller reports a light over the reservoir. Caller declined to describe \
colour. Deputy attended 21:40, nothing observed.";

fn cite(text: &str, phrase: &str) -> Result<Range<usize>, Box<dyn Error>> {
    let start = text.find(phrase).ok_or("phrase not in fixture text")?;
    Ok(start..start + phrase.len())
}

fn said<'a>(
    text: &str,
    aspect: &'a str,
    value: &'a str,
    phrase: &str,
) -> Result<DetailAssertion<'a>, Box<dyn Error>> {
    Ok(DetailAssertion {
        aspect,
        value: CodedValue::Reported(value),
        wording: cite(text, phrase)?,
    })
}

fn unknown<'a>(
    text: &str,
    aspect: &'a str,
    phrase: &str,
) -> Result<DetailAssertion<'a>, Box<dyn Error>> {
    Ok(DetailAssertion {
        aspect,
        value: CodedValue::StatedUnknown,
        wording: cite(text, phrase)?,
    })
}

struct Fixture {
    news: Vec<DetailAssertion<'static>>,
    copied: Vec<DetailAssertion<'static>>,
    a1: Vec<DetailAssertion<'static>>,
    a2: Vec<DetailAssertion<'static>>,
    b: Vec<DetailAssertion<'static>>,
    police: Vec<DetailAssertion<'static>>,
    police_account: [OpaqueId; 1],
}

fn fixture() -> Result<Fixture, Box<dyn Error>> {
    Ok(Fixture {
        news: vec![
            said(NEWS, "colour", "orange", "orange light")?,
            said(NEWS, "sound", "silent", "silent")?,
            said(NEWS, "location", "reservoir", "over the reservoir")?,
            said(NEWS, "motion", "hovering", "hovering")?,
        ],
        copied: vec![
            said(COPIED, "colour", "orange", "orange light")?,
            said(COPIED, "sound", "silent", "silent")?,
            said(COPIED, "location", "reservoir", "over the reservoir")?,
            said(COPIED, "motion", "hovering", "hovering")?,
            said(COPIED, "identity", "drone", "I think it was a drone")?,
        ],
        a1: vec![
            said(WITNESS_A1, "colour", "orange", "an orange light")?,
            said(WITNESS_A1, "sound", "silent", "completely silent")?,
            said(WITNESS_A1, "location", "reservoir", "by the reservoir")?,
            said(WITNESS_A1, "duration", "about 2 min", "about two minutes")?,
        ],
        a2: vec![
            said(WITNESS_A2, "colour", "red", "a red light")?,
            said(WITNESS_A2, "sound", "faint hum", "Maybe a faint hum")?,
            said(WITNESS_A2, "location", "reservoir", "by the reservoir")?,
            said(WITNESS_A2, "duration", "about 2 min", "about two minutes")?,
        ],
        b: vec![
            said(WITNESS_B, "colour", "orange", "an orange glow")?,
            said(WITNESS_B, "location", "reservoir", "above the reservoir")?,
            unknown(WITNESS_B, "size", "I could not tell how big it was")?,
            said(WITNESS_B, "sound", "silent", "No sound that I noticed")?,
        ],
        // No sound wording: an omitted field, not an observed silence.
        police: vec![
            said(POLICE, "location", "reservoir", "over the reservoir")?,
            unknown(POLICE, "colour", "Caller declined to describe colour")?,
        ],
        police_account: [OpaqueId::new("police-report-17")?],
    })
}

#[allow(clippy::too_many_arguments)]
fn source<'a>(
    account: &str,
    revision: u32,
    class: SourceClass,
    exposure: ExplanationExposure,
    text: TextCapture<'a>,
    assertions: &'a [DetailAssertion<'a>],
    derived_from: &'a [OpaqueId],
) -> Result<StatementSource<'a>, Box<dyn Error>> {
    Ok(StatementSource {
        account: OpaqueId::new(account)?,
        revision,
        version: VersionId::new(format!("{account}-r{revision}"))?,
        class,
        origin: AccountOrigin::Reported,
        exposure,
        attribution: "Synthetic #138 fixture",
        text,
        assertions,
        derived_from,
    })
}

fn sources(f: &Fixture) -> Result<Vec<StatementSource<'_>>, Box<dyn Error>> {
    use ExplanationExposure::{AfterExplanations, NotExposedInWorkflow, Unknown};
    use SourceClass::{NewsReport, PoliceReport, Testimony};
    Ok(vec![
        source(
            "news-1",
            1,
            NewsReport,
            Unknown,
            TextCapture::Complete(NEWS),
            &f.news,
            &f.police_account,
        )?,
        source(
            "witness-c",
            1,
            Testimony,
            Unknown,
            TextCapture::Complete(COPIED),
            &f.copied,
            &[],
        )?,
        source(
            "witness-a",
            1,
            Testimony,
            NotExposedInWorkflow,
            TextCapture::Complete(WITNESS_A1),
            &f.a1,
            &[],
        )?,
        source(
            "witness-a",
            2,
            Testimony,
            AfterExplanations,
            TextCapture::Complete(WITNESS_A2),
            &f.a2,
            &[],
        )?,
        source(
            "witness-b",
            1,
            Testimony,
            NotExposedInWorkflow,
            TextCapture::Complete(WITNESS_B),
            &f.b,
            &[],
        )?,
        source(
            "police-report-17",
            1,
            PoliceReport,
            Unknown,
            TextCapture::Partial {
                text: POLICE,
                reason: "page 2 of the incident log not retained",
            },
            &f.police,
            &[],
        )?,
    ])
}

fn request<'a>(
    sources: &'a [StatementSource<'a>],
    representative: Representative,
    dismissals: &'a [CandidateDismissal<'a>],
) -> Result<ComparisonRequest<'a>, Box<dyn Error>> {
    Ok(ComparisonRequest {
        coding_version: VersionId::new("coding-scheme-v1")?,
        coded_by: ActorId::new("analyst-fixture")?,
        parameters: ComparisonParameters {
            representative,
            ..ComparisonParameters::default()
        },
        sources,
        dismissals,
    })
}

#[test]
fn copied_wording_lowers_independence_and_variants_omissions_and_drift_stay_visible()
-> Result<(), Box<dyn Error>> {
    let f = fixture()?;
    let sources = sources(&f)?;
    let req = request(&sources, Representative::Latest, &[])?;
    let result = compare_statements(&req, || Ok(()))?;

    assert_eq!(result.method.id, METHOD_ID);
    assert!(result.method.overlap_caveat.contains("not a probability"));
    assert!(
        result
            .method
            .candidate_groups_caveat
            .contains("not a bound")
    );
    assert!(!result.contains_simulated);

    // News declares it draws on the police report; witness-c copies the news.
    // Only the candidate-adjusted grouping merges the copy.
    assert_eq!(
        (result.account_count, result.reported_account_count),
        (5, 5)
    );
    assert_eq!(result.declared.reported_group_count, 4);
    assert_eq!(result.candidate_adjusted.reported_group_count, 3);
    let [copy] = result.copy_candidates.as_slice() else {
        return Err("expected exactly one copy candidate".into());
    };
    assert_eq!((copy.a, copy.b), (0, 1));
    assert!(copy.words >= 12);
    assert_eq!(&NEWS[copy.a_range.clone()], copy.a_text);
    assert_eq!(&COPIED[copy.b_range.clone()], copy.b_text);
    assert!(copy.a_text.starts_with("Residents reported"));
    assert_eq!(copy.dismissal, None);
    let news_group = &result.candidate_adjusted.groups[result.sources[0].candidate_adjusted_group];
    assert_eq!(news_group.accounts.len(), 3);
    assert!(
        news_group
            .reasons
            .iter()
            .any(|r| matches!(r, DependencyReason::SharedWording { candidate: 0 }))
    );
    assert!(
        news_group
            .reasons
            .iter()
            .any(|r| matches!(r, DependencyReason::DeclaredDerivation { source: 0, .. }))
    );

    // Same-account revisions are drift, not copying; matrix stays exact.
    let revision_pair = result
        .pairs
        .iter()
        .find(|p| (p.a, p.b) == (2, 3))
        .ok_or("pair")?;
    assert_eq!(revision_pair.copy_search, CopySearch::SameAccount);
    assert!(
        matches!(revision_pair.overlap, OverlapMeasure::ShingleJaccard(r) if r.numerator > 0 && r.numerator < r.denominator)
    );
    assert_eq!(result.pairs.len(), 15);

    let aspect = |key: &str| {
        result
            .aspects
            .iter()
            .find(|a| a.aspect == key)
            .ok_or("aspect")
    };
    // Location: five accounts, four declared groups, three after the copy.
    let location = aspect("location")?;
    assert_eq!(location.status, AspectStatus::Converging);
    let v = &location.variants[0];
    assert_eq!(
        (v.accounts, v.declared_groups, v.candidate_adjusted_groups),
        (5, 4, 3)
    );
    // Motion appears only in the news report and its copy.
    assert_eq!(
        aspect("motion")?.status,
        AspectStatus::RepeatedWithinOneGroup
    );
    assert_eq!(aspect("identity")?.status, AspectStatus::Unique);
    assert_eq!(aspect("size")?.status, AspectStatus::StatedUnknownOnly);
    // Colour conflicts (latest witness-a says red); the police "declined" is an
    // explicit unknown, kept apart from the variants and from omissions.
    let colour = aspect("colour")?;
    assert_eq!(colour.status, AspectStatus::Conflicting);
    let values: Vec<_> = colour
        .variants
        .iter()
        .map(|v| {
            (
                v.value,
                v.accounts,
                v.declared_groups,
                v.candidate_adjusted_groups,
            )
        })
        .collect();
    assert_eq!(values, [("orange", 3, 3, 2), ("red", 1, 1, 1)]);
    assert_eq!(colour.stated_unknown_by, [5]);
    assert!(colour.not_coded_in.is_empty());
    // Police never mention sound: an omission in a partial text, not silence.
    let sound = aspect("sound")?;
    assert_eq!(sound.status, AspectStatus::Conflicting);
    assert_eq!(sound.not_coded_in.len(), 1);
    assert_eq!(
        (sound.not_coded_in[0].source, sound.not_coded_in[0].coverage),
        (5, TextCoverage::Partial)
    );
    assert_eq!(
        colour
            .reports
            .iter()
            .find(|r| r.source == 3)
            .ok_or("report")?
            .wording_text,
        "a red light"
    );

    // Drift of witness-a after explanations: colour and sound changed.
    let [drift] = result.drift.as_slice() else {
        return Err("expected one drift step".into());
    };
    assert_eq!(
        (drift.from, drift.to, drift.to_exposure),
        (2, 3, ExplanationExposure::AfterExplanations)
    );
    let changed: Vec<_> = drift
        .changes
        .iter()
        .map(|c| (c.aspect, c.before.clone(), c.after.clone()))
        .collect();
    assert_eq!(
        changed,
        [
            (
                "colour",
                vec![CodedValue::Reported("orange")],
                vec![CodedValue::Reported("red")]
            ),
            (
                "sound",
                vec![CodedValue::Reported("silent")],
                vec![CodedValue::Reported("faint hum")]
            ),
        ]
    );
    assert_eq!(drift.unchanged_aspects, 2);
    assert_eq!(drift.text_identical, Some(false));

    assert!(result.is_partial());
    assert_eq!(result.limitations, [Limitation::TextPartial { source: 5 }]);

    // A reviewer dismisses the candidate (say, both quote one public notice): it
    // stays listed but no longer merges groups, so motion now converges.
    let dismissal = [CandidateDismissal {
        a_version: VersionId::new("witness-c-r1")?,
        a_range: copy.b_range.clone(),
        b_version: VersionId::new("news-1-r1")?,
        b_range: copy.a_range.clone(),
        reviewed_by: ActorId::new("reviewer-fixture")?,
        reason: "both quote the same public appeal",
    }];
    let dismissed = compare_statements(
        &request(&sources, Representative::Latest, &dismissal)?,
        || Ok(()),
    )?;
    assert_eq!(dismissed.copy_candidates[0].dismissal, Some(0));
    assert_eq!(dismissed.candidate_adjusted.reported_group_count, 4);
    let motion = dismissed
        .aspects
        .iter()
        .find(|a| a.aspect == "motion")
        .ok_or("aspect")?;
    assert_eq!(motion.status, AspectStatus::Converging);

    // The initial account is selectable; colour then has one variant across groups.
    let initial = compare_statements(&request(&sources, Representative::Initial, &[])?, || Ok(()))?;
    let colour = initial
        .aspects
        .iter()
        .find(|a| a.aspect == "colour")
        .ok_or("aspect")?;
    assert_eq!(colour.status, AspectStatus::Converging);
    Ok(())
}

#[test]
fn malformed_citations_and_cancellation_return_no_result() -> Result<(), Box<dyn Error>> {
    let f = fixture()?;
    let mut sources = sources(&f)?;
    // Byte 1 of "é" is not a character boundary.
    let accented = [DetailAssertion {
        aspect: "colour",
        value: CodedValue::Reported("orange"),
        wording: 1..2,
    }];
    sources[4].text = TextCapture::Complete("é orange");
    sources[4].assertions = &accented;
    assert_eq!(
        compare_statements(&request(&sources, Representative::Latest, &[])?, || Ok(())),
        Err(ComparisonError::InvalidWording {
            source: 4,
            assertion: 0
        })
    );
    // A whitespace-only citation is refused as coded wording.
    let blank = [DetailAssertion {
        aspect: "colour",
        value: CodedValue::Reported("orange"),
        wording: 2..3,
    }];
    sources[4].assertions = &blank;
    assert_eq!(
        compare_statements(&request(&sources, Representative::Latest, &[])?, || Ok(())),
        Err(ComparisonError::InvalidWording {
            source: 4,
            assertion: 0
        })
    );
    // A citation into unavailable text cannot be checked.
    sources[4].text = TextCapture::Unavailable {
        reason: "restricted after retrieval",
    };
    assert_eq!(
        compare_statements(&request(&sources, Representative::Latest, &[])?, || Ok(())),
        Err(ComparisonError::WordingWithoutText {
            source: 4,
            assertion: 0
        })
    );
    let sources = self::sources(&f)?;
    assert_eq!(
        compare_statements(&request(&sources, Representative::Latest, &[])?, || Err(
            StopReason::Cancelled
        )),
        Err(ComparisonError::Stopped(StopReason::Cancelled))
    );
    Ok(())
}

#[test]
fn simulated_support_and_unsegmented_text_are_not_presented_as_corroboration()
-> Result<(), Box<dyn Error>> {
    let f = fixture()?;
    let mut sources = sources(&f)?;
    // A simulated police report: the news report declares it draws only on it,
    // so neither counts as reported support.
    sources[5].origin = AccountOrigin::Simulated;
    let result = compare_statements(&request(&sources, Representative::Latest, &[])?, || Ok(()))?;
    assert!(result.contains_simulated);
    assert_eq!(result.reported_account_count, 3);
    assert!(!result.sources[0].counts_as_reported);
    assert_eq!(result.declared.reported_group_count, 3);
    let location = result
        .aspects
        .iter()
        .find(|a| a.aspect == "location")
        .ok_or("aspect")?;
    let v = &location.variants[0];
    assert_eq!(
        (v.accounts, v.reported_accounts, v.simulated_accounts),
        (5, 3, 1)
    );

    // Identical text without word separators: not a silent complete miss.
    const HAN: &str = "\u{76ee}\u{51fb}\u{8005}\u{8bf4}\u{4ed6}\u{5728}\u{6c34}\u{5e93}\u{4e0a}\u{7a7a}\u{770b}\u{5230}\u{4e00}\u{4e2a}\u{6a59}\u{8272}\u{7684}\u{5149}\u{70b9}\u{9759}\u{6b62}\u{4e0d}\u{52a8}";
    let mut pair = sources[1..3].to_vec();
    pair[0].text = TextCapture::Complete(HAN);
    pair[1].text = TextCapture::Complete(HAN);
    pair[0].assertions = &[];
    pair[1].assertions = &[];
    let result = compare_statements(&request(&pair, Representative::Latest, &[])?, || Ok(()))?;
    assert!(result.is_partial());
    assert!(
        result
            .limitations
            .iter()
            .any(|l| matches!(l, Limitation::UnsegmentedText { source: 0, .. }))
    );
    assert_eq!(result.pairs[0].copy_search, CopySearch::UnsegmentedText);
    assert_eq!(result.pairs[0].overlap, OverlapMeasure::TooShort);
    Ok(())
}
