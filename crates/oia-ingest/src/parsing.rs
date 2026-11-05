use crate::collection::{Check, Entry, Error, MAX_ITEMS, Parsed, hash};
use crate::transport::public_url;
use quick_xml::{Reader, events::Event};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

fn malformed(message: impl ToString) -> Error {
    Error::new("malformed", message.to_string())
}
fn field(value: &str, limit: usize) -> Result<(), Error> {
    if value.len() > limit || value.contains('\0') {
        return Err(malformed(
            "A source field exceeded its byte limit or contained NUL.",
        ));
    }
    Ok(())
}

fn entry(
    source: &str,
    base_url: &str,
    fields: &BTreeMap<String, String>,
    item: &[u8],
    start: usize,
    end: usize,
    response: &str,
) -> Result<Option<Entry>, Error> {
    let value = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| fields.get(*key).filter(|value| !value.trim().is_empty()))
            .cloned()
    };
    let Some(title) = value(&["title"]) else {
        return Ok(None);
    };
    field(&title, 512)?;
    let url = value(&["link"]).and_then(|value| {
        let resolved = reqwest::Url::parse(base_url)
            .ok()?
            .join(value.trim())
            .ok()?;
        let mut destination = resolved.clone();
        destination.set_fragment(None);
        public_url(destination.as_str())
            .ok()
            .map(|_| resolved.to_string())
    });
    let Some(provider_id) = value(&["id", "guid"]).or_else(|| url.clone()) else {
        return Ok(None);
    };
    field(&provider_id, 2048)?;
    let excerpt = value(&["description", "summary", "content"]);
    let published_time = value(&["pubDate", "published"]);
    let updated_time = value(&["updated"]);
    if let Some(value) = &excerpt {
        field(value, 64 * 1024)?;
    }
    for value in [&published_time, &updated_time].into_iter().flatten() {
        field(value, 160)?;
    }
    let item_sha256 = hash(item);
    let identity = format!("{source}\n{provider_id}");
    Ok(Some(Entry {
        record_id: format!("item-{}", hash(identity.as_bytes())),
        version_id: format!(
            "itemv-{}",
            hash(format!("{identity}\n{item_sha256}").as_bytes())
        ),
        provider_id,
        title,
        url,
        excerpt,
        published_time,
        updated_time,
        archive_covering_dates: None,
        archive_reference: None,
        response_sha256: response.into(),
        item_sha256,
        byte_start: start,
        byte_end: end,
    }))
}

/// RSS 2.0 and Atom metadata only. DTDs are refused and no external entity, link, enclosure
/// or HTML is fetched. Every accepted item's range addresses the retained response bytes.
pub(crate) fn feed(
    source: &str,
    base_url: &str,
    bytes: &[u8],
    response: &str,
    check: Check<'_>,
) -> Result<Parsed, Error> {
    std::str::from_utf8(bytes)
        .map_err(|_| malformed("Only UTF-8 RSS/Atom feeds are supported."))?;
    let mut reader = Reader::from_reader(bytes);
    let mut path: Vec<String> = Vec::new();
    let mut root = None;
    let mut roots = 0;
    let mut item_depth = None;
    let mut item_start = 0;
    let mut fields = BTreeMap::<String, String>::new();
    let mut entries = Vec::new();
    let mut supplied = 0;
    let mut excluded = 0;
    let mut identities = BTreeSet::new();
    loop {
        check()?;
        let start = reader.buffer_position() as usize;
        match reader.read_event().map_err(malformed)? {
            Event::DocType(_) => {
                return Err(malformed("DTD/entity declarations are not supported."));
            }
            Event::Start(tag) => {
                let name = tag.local_name().as_ref().to_owned();
                if path.is_empty() {
                    roots += 1;
                    root = Some(name.clone());
                }
                if roots > 1 || path.len() >= 32 {
                    return Err(malformed(
                        "The feed has multiple roots or exceeds 32 levels of nesting.",
                    ));
                }
                path.push(name.clone());
                let is_item = (root.as_deref() == Some("rss")
                    && path.as_slice() == ["rss", "channel", "item"])
                    || (root.as_deref() == Some("feed") && path.as_slice() == ["feed", "entry"]);
                if is_item {
                    supplied += 1;
                    item_depth = Some(path.len());
                    item_start = start;
                    fields.clear();
                }
                if name == "link" && item_depth.is_some_and(|d| path.len() == d + 1) {
                    atom_link(&tag, &mut fields)?;
                }
            }
            Event::Empty(tag) => {
                if path.is_empty() {
                    roots += 1;
                    root = Some(tag.local_name().as_ref().to_owned());
                    if roots > 1 {
                        return Err(malformed("The feed has multiple roots."));
                    }
                }
                if tag.local_name().as_ref() == "link"
                    && item_depth.is_some_and(|d| path.len() == d)
                {
                    atom_link(&tag, &mut fields)?;
                }
            }
            Event::Text(text) => {
                if path.is_empty() && !text.as_ref().trim().is_empty() {
                    return Err(malformed("Unexpected text outside the feed root."));
                }
                if item_depth.is_some_and(|d| path.len() == d + 1) {
                    let decoded = text.as_ref();
                    // General references are separate events in quick-xml 0.42.
                    append(&path, &mut fields, decoded)?;
                }
            }
            Event::CData(text) => {
                if item_depth.is_some_and(|d| path.len() == d + 1) {
                    append(&path, &mut fields, text.as_ref())?;
                }
            }
            Event::GeneralRef(reference) => {
                if item_depth.is_some_and(|d| path.len() == d + 1) {
                    let value = reference.as_ref();
                    let escaped = format!("&{value};");
                    let decoded = quick_xml::escape::unescape(&escaped).map_err(malformed)?;
                    append(&path, &mut fields, &decoded)?;
                }
            }
            Event::End(_) => {
                if item_depth == Some(path.len()) {
                    let end = reader.buffer_position() as usize;
                    if entries.len() < MAX_ITEMS {
                        let raw = bytes
                            .get(item_start..end)
                            .ok_or_else(|| malformed("Invalid item byte range."))?;
                        match entry(source, base_url, &fields, raw, item_start, end, response)? {
                            Some(entry) if identities.insert(entry.record_id.clone()) => {
                                entries.push(entry)
                            }
                            _ => excluded += 1,
                        }
                    } else {
                        excluded += 1;
                    }
                    item_depth = None;
                }
                path.pop();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !path.is_empty() || roots != 1 || !matches!(root.as_deref(), Some("rss" | "feed")) {
        return Err(malformed("Expected a complete RSS 2.0 or Atom document."));
    }
    Ok(Parsed {
        entries,
        supplied,
        excluded,
        note: format!(
            "Current feed document only; {supplied} items supplied, {excluded} excluded by the 30-item limit, duplicate identity or missing title/identity. Feed excerpts only, not full articles or complete news coverage. Feed HTML is retained as text; no source scripts or linked resources are loaded."
        ),
    })
}

fn append(path: &[String], fields: &mut BTreeMap<String, String>, text: &str) -> Result<(), Error> {
    let Some(name) = path.last() else {
        return Ok(());
    };
    if ![
        "title",
        "link",
        "id",
        "guid",
        "description",
        "summary",
        "content",
        "pubDate",
        "published",
        "updated",
    ]
    .contains(&name.as_str())
    {
        return Ok(());
    }
    let value = fields.entry(name.clone()).or_default();
    if value.len() + text.len() > 64 * 1024 {
        return Err(malformed("Feed field exceeds 64 KiB."));
    }
    value.push_str(text);
    Ok(())
}

fn atom_link(
    tag: &quick_xml::events::BytesStart<'_>,
    fields: &mut BTreeMap<String, String>,
) -> Result<(), Error> {
    let mut href = None;
    let mut rel = None;
    for attr in tag.attributes() {
        let attr = attr.map_err(malformed)?;
        match attr.key.local_name().as_ref() {
            "href" => {
                href = Some(
                    attr.normalized_value(quick_xml::XmlVersion::Explicit1_0)
                        .map_err(malformed)?
                        .into_owned(),
                )
            }
            "rel" => {
                rel = Some(
                    attr.normalized_value(quick_xml::XmlVersion::Explicit1_0)
                        .map_err(malformed)?
                        .into_owned(),
                )
            }
            _ => {}
        }
    }
    if rel.as_deref().is_none_or(|value| value == "alternate")
        && let Some(href) = href
    {
        fields.insert("link".into(), href);
    }
    Ok(())
}

#[derive(Deserialize)]
struct ArchiveInput<'a> {
    #[serde(borrow)]
    records: Vec<&'a serde_json::value::RawValue>,
    count: usize,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveRecord {
    id: String,
    title: String,
    description: Option<String>,
    covering_dates: Option<String>,
    reference: Option<String>,
}

pub(crate) fn archive(
    source: &str,
    bytes: &[u8],
    response: &str,
    check: Check<'_>,
) -> Result<Parsed, Error> {
    let input: ArchiveInput<'_> = serde_json::from_slice(bytes).map_err(malformed)?;
    if input.records.len() > 1000 || input.count < input.records.len() {
        return Err(malformed(
            "Archive response has inconsistent counts or exceeds 1,000 records.",
        ));
    }
    let supplied = input.records.len();
    let mut entries = Vec::new();
    let mut identities = BTreeSet::new();
    for raw in input.records.iter().take(MAX_ITEMS) {
        check()?;
        let record: ArchiveRecord = serde_json::from_str(raw.get()).map_err(malformed)?;
        if record.id.is_empty()
            || record.id.len() > 128
            || !record
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(malformed("Invalid archive record identifier."));
        }
        for value in [&record.covering_dates, &record.reference]
            .into_iter()
            .flatten()
        {
            field(value, 512)?;
        }
        let mut fields = BTreeMap::from([
            ("title".into(), record.title),
            ("id".into(), record.id.clone()),
            (
                "link".into(),
                format!(
                    "https://discovery.nationalarchives.gov.uk/details/r/{}",
                    record.id
                ),
            ),
        ]);
        if let Some(description) = record.description {
            fields.insert("description".into(), description);
        }
        // RawValue is borrowed directly from the input; retain its exact byte address/range.
        let start = raw
            .get()
            .as_ptr()
            .addr()
            .checked_sub(bytes.as_ptr().addr())
            .ok_or_else(|| malformed("Invalid archive anchor."))?;
        let end = start
            .checked_add(raw.get().len())
            .ok_or_else(|| malformed("Invalid archive anchor."))?;
        let item = bytes
            .get(start..end)
            .ok_or_else(|| malformed("Archive anchor is outside its response."))?;
        if let Some(mut entry) = entry(
            source,
            "https://discovery.nationalarchives.gov.uk/",
            &fields,
            item,
            start,
            end,
            response,
        )? && identities.insert(entry.record_id.clone())
        {
            entry.archive_reference = record.reference;
            entry.archive_covering_dates = record.covering_dates;
            entries.push(entry);
        }
    }
    let excluded = supplied.saturating_sub(entries.len());
    Ok(Parsed {
        entries,
        supplied,
        excluded,
        note: format!(
            "First page only: {supplied} catalogue records supplied of {} matches; {excluded} excluded. Descriptions and references to historical UFO-report files, not full testimony, event counts, verified sightings or complete archive coverage. Covering dates belong to the file, not necessarily the reported event.",
            input.count
        ),
    })
}
