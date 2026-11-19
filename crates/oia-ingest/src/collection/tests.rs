//! Three focused contracts: original/version preservation, bounded untrusted parsing, and
//! destination isolation. Live provider availability is checked separately by collect-public.
use super::*;
use crate::transport::{public_ip, public_url};
use std::{collections::VecDeque, net::IpAddr};

const RSS: &[u8] = b"<rss version=\"2.0\"><channel><item><guid>witness-1</guid><title>Two &amp; three lights</title><link>https://example.org/report</link><description><![CDATA[<p>Exact reported wording.</p>]]></description></item></channel></rss>";

struct Fixture {
    responses: VecDeque<Response>,
    requests: usize,
}
impl Transport for Fixture {
    fn get(
        &mut self,
        _request: crate::transport::Request<'_>,
        check: Check<'_>,
    ) -> Result<Response, Error> {
        check()?;
        self.requests += 1;
        self.responses
            .pop_front()
            .ok_or_else(|| Error::new("fixture", "Unexpected request"))
    }
}
fn response(status: u16, body: &[u8]) -> Response {
    Response {
        status,
        body: body.to_vec(),
        etag: None,
        last_modified: None,
        retry_after: None,
        cache_max_age: None,
        no_store: false,
    }
}
fn registry() -> Registry {
    Registry {
        sources: vec![Registry::default().sources.remove(0)],
    }
}

#[test]
fn retains_exact_originals_revisions_and_previous_results_on_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let revised = String::from_utf8(RSS.to_vec())?.replace(
        "Exact reported wording.",
        "A correction, still an assertion.",
    );
    let mut fixture = Fixture {
        responses: [
            Response {
                etag: Some("version-1".into()),
                ..response(200, RSS)
            },
            response(304, b""),
            response(200, revised.as_bytes()),
            Response {
                retry_after: Some(3600),
                ..response(429, b"")
            },
            response(200, b"<rss><channel><item>"),
        ]
        .into(),
        requests: 0,
    };
    let mut registry = registry();
    registry.refresh(Category::News, "", &mut fixture, &|| Ok(()))?;
    let first = registry.sources[0]
        .current
        .clone()
        .ok_or("missing response")?;
    assert_eq!(first.original(), RSS);
    assert_eq!(first.info.sha256, hash(RSS));
    let entry = first.entries.first().ok_or("missing item")?;
    assert_eq!(entry.title, "Two & three lights");
    assert_eq!(
        entry.item_sha256,
        hash(&RSS[entry.byte_start..entry.byte_end])
    );
    assert!(entry.published_time.is_none());
    let version = VersionId::new(entry.version_id.clone())?;
    let extracted = first
        .evidence()
        .version(&version)
        .ok_or("missing evidence")?;
    assert_eq!(
        extracted.fields().origin,
        OriginClass::ConventionalDerivative
    );
    assert_eq!(extracted.fields().review, ReviewStatus::Unreviewed);
    assert_eq!(extracted.fields().provenance.inputs().len(), 1);
    registry.refresh(Category::News, "", &mut fixture, &|| Ok(()))?;
    assert_eq!(
        fixture.requests, 1,
        "cadence must suppress an immediate retry"
    );
    registry.sources[0].next_attempt_ms = 0;
    registry.refresh(Category::News, "", &mut fixture, &|| Ok(()))?;
    assert_eq!(
        registry.sources[0].history.len(),
        1,
        "identical retries are idempotent"
    );
    registry.sources[0].next_attempt_ms = 0;
    registry.refresh(Category::News, "", &mut fixture, &|| Ok(()))?;
    let latest = registry.sources[0]
        .current
        .clone()
        .ok_or("missing revision")?;
    assert_eq!(latest.entries[0].record_id, entry.record_id);
    assert_ne!(latest.entries[0].version_id, entry.version_id);
    assert_eq!(
        first.original(),
        RSS,
        "old originals must never be rewritten"
    );
    registry.sources[0].next_attempt_ms = 0;
    registry.refresh(Category::News, "", &mut fixture, &|| Ok(()))?;
    assert_eq!(registry.sources[0].outcome, "rate_limited");
    assert!(
        registry.sources[0].next_attempt_ms
            >= registry.sources[0].last_attempt_ms.ok_or("missing time")? + 3_600_000
    );
    registry.sources[0].next_attempt_ms = 0;
    registry.refresh(Category::News, "", &mut fixture, &|| Ok(()))?;
    assert_eq!(registry.sources[0].outcome, "malformed");
    assert_eq!(
        registry.sources[0]
            .current
            .as_ref()
            .ok_or("lost previous")?
            .original(),
        latest.original()
    );
    let mut staged = registry.clone();
    assert_eq!(
        staged
            .refresh(Category::News, "", &mut fixture, &|| Err(Error::new(
                "cancelled",
                "stopped"
            )))
            .err()
            .ok_or("cancellation ignored")?
            .code,
        "cancelled"
    );
    assert_eq!(registry.sources[0].history.len(), 2);
    fixture.responses.push_back(Response {
        no_store: true,
        ..response(200, RSS)
    });
    registry.sources[0].next_attempt_ms = 0;
    registry.refresh(Category::News, "", &mut fixture, &|| Ok(()))?;
    assert_eq!(registry.sources[0].outcome, "reference_only");
    assert_eq!(
        registry.sources[0].history.len(),
        2,
        "no-store response must not be retained"
    );
    Ok(())
}

#[test]
fn bounded_parsers_keep_archive_anchors_and_unknowns() -> Result<(), Box<dyn std::error::Error>> {
    let atom = b"<feed xmlns=\"http://www.w3.org/2005/Atom\"><entry><id>urn:account:1</id><title>Account</title><link rel=\"alternate\" href=\"https://example.org/account\"/><summary>&lt;script&gt;data only&lt;/script&gt;</summary></entry></feed>";
    let parsed = parsing::feed(
        "feed",
        "https://example.org/feed.xml",
        atom,
        &hash(atom),
        &|| Ok(()),
    )?;
    assert_eq!(
        parsed.entries[0].excerpt.as_deref(),
        Some("<script>data only</script>")
    );
    assert_eq!(
        parsed.entries[0].url.as_deref(),
        Some("https://example.org/account")
    );
    assert!(
        parsing::feed(
            "feed",
            "https://example.org/feed.xml",
            b"<feed/>",
            "hash",
            &|| Ok(())
        )?
        .entries
        .is_empty()
    );
    for bytes in [
        b"<!DOCTYPE rss [<!ENTITY x SYSTEM 'file:///private'>]><rss/>".as_slice(),
        b"<html><item/></html>",
        b"junk<rss/>",
    ] {
        assert!(
            parsing::feed(
                "feed",
                "https://example.org/feed.xml",
                bytes,
                "hash",
                &|| Ok(())
            )
            .is_err()
        );
    }
    let many = format!(
        "<rss><channel>{}</channel></rss>",
        (0..35)
            .map(|n| format!("<item><guid>{n}</guid><title>T</title></item>"))
            .collect::<String>()
    );
    let parsed = parsing::feed(
        "feed",
        "https://example.org/feed.xml",
        many.as_bytes(),
        "hash",
        &|| Ok(()),
    )?;
    assert_eq!(
        (parsed.entries.len(), parsed.supplied, parsed.excluded),
        (30, 35, 5)
    );
    let bytes = br#"{"records":[{"id":"C1","title":"Reported UFO","description":"Catalogue only","coveringDates":"1950-1980","reference":"AIR 1"}],"count":50}"#;
    let archive = parsing::archive("tna", bytes, &hash(bytes), &|| Ok(()))?;
    let record = &archive.entries[0];
    assert_eq!(
        record.item_sha256,
        hash(&bytes[record.byte_start..record.byte_end])
    );
    assert_eq!(record.archive_covering_dates.as_deref(), Some("1950-1980"));
    assert!(record.published_time.is_none());
    assert!(archive.note.contains("50 matches"));
    let mut registry = registry();
    let mut oversized = Fixture {
        responses: [response(200, &vec![b'x'; MAX_BYTES + 1])].into(),
        requests: 0,
    };
    registry.refresh(Category::News, "", &mut oversized, &|| Ok(()))?;
    assert_eq!(registry.sources[0].outcome, "byte_limit");
    assert!(registry.sources[0].current.is_none());
    let source = Registry::default().sources.pop().ok_or("missing archive")?;
    let mut archive_registry = Registry {
        sources: vec![source],
    };
    let mut fixture = Fixture {
        responses: [response(200, bytes), response(200, bytes)].into(),
        requests: 0,
    };
    archive_registry.refresh(Category::Reports, "", &mut fixture, &|| Ok(()))?;
    let first = archive_registry.sources[0]
        .current
        .clone()
        .ok_or("missing first query")?;
    archive_registry.sources[0].next_attempt_ms = 0;
    archive_registry.refresh(Category::Reports, "Rendlesham", &mut fixture, &|| Ok(()))?;
    let second = archive_registry.sources[0]
        .current
        .clone()
        .ok_or("missing second query")?;
    assert_eq!(first.info.sha256, second.info.sha256);
    assert_ne!(
        first.entries[0].version_id, second.entries[0].version_id,
        "different query provenance must not reuse an extraction version"
    );
    assert_eq!(
        archive_registry
            .snapshot("tna-ufo", &second.info.sha256, &second.info.request_url)?
            .info
            .request_url,
        second.info.request_url
    );
    Ok(())
}

#[test]
fn refuses_private_network_credentials_and_special_addresses()
-> Result<(), Box<dyn std::error::Error>> {
    for raw in [
        "http://example.org/feed",
        "https://127.0.0.1/",
        "https://169.254.169.254/",
        "https://[::1]/",
        "https://[::ffff:127.0.0.1]/",
        "https://name:password@example.org/",
        "https://example.org:444/",
        "https://service.local/",
        "https://localhost./",
        "file:///private",
    ] {
        assert!(public_url(raw).is_err(), "accepted {raw}");
    }
    for raw in [
        "10.0.0.1",
        "100.64.0.1",
        "172.31.0.1",
        "192.168.0.1",
        "192.0.2.1",
        "198.18.0.1",
        "203.0.113.1",
        "224.0.0.1",
        "fc00::1",
        "fe80::1",
        "2001:db8::1",
        "2002:7f00:1::",
    ] {
        assert!(!public_ip(raw.parse::<IpAddr>()?), "accepted {raw}");
    }
    for raw in ["192.0.66.108", "8.8.8.8", "2a04:fa87:fffd::c000:426c"] {
        assert!(public_ip(raw.parse()?));
    }
    assert!(public_url("https://feeds.bbci.co.uk/news/world/rss.xml").is_ok());
    Ok(())
}
