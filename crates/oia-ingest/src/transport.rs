//! HTTPS GET only. DNS results are checked and pinned; redirects and proxies are disabled.
use crate::collection::{Check, Error, MAX_BYTES};
use reqwest::{Client, Url, redirect::Policy};
use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    sync::mpsc::{self, SyncSender},
    time::{Duration, Instant, SystemTime},
};

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub retry_after: Option<u64>,
    pub cache_max_age: Option<u64>,
    pub no_store: bool,
}

#[derive(Debug)]
pub struct Request<'a> {
    pub url: &'a str,
    pub cadence_seconds: u64,
    pub etag: Option<&'a str>,
    pub last_modified: Option<&'a str>,
}

/// Per-adapter budgets within the shared transport's hard ceilings.
#[derive(Clone, Copy, Debug)]
pub struct HttpPolicy {
    pub accept: &'static str,
    pub expected_content_type: Option<&'static str>,
    pub max_bytes: usize,
    pub deadline: Duration,
    pub connect_timeout: Duration,
    pub endpoint_budget: usize,
    pub no_cache: bool,
}
impl Default for HttpPolicy {
    fn default() -> Self {
        Self {
            accept: "application/rss+xml, application/atom+xml, application/json, application/xml, text/xml",
            expected_content_type: None,
            max_bytes: MAX_BYTES,
            deadline: Duration::from_secs(8),
            connect_timeout: Duration::from_secs(3),
            endpoint_budget: 6,
            no_cache: false,
        }
    }
}

pub trait Transport {
    fn get(&mut self, request: Request<'_>, check: Check<'_>) -> Result<Response, Error>;
}

fn error(code: &'static str, message: &str) -> Error {
    Error::new(code, message)
}

/// Conservative public-address policy, including IPv4-mapped IPv6 and transition ranges.
#[must_use]
pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192
                    && ((b == 0 && (c == 0 || c == 2)) || b == 168 || (b == 88 && c == 99)))
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            (s[0] & 0xe000) == 0x2000
                && s[0] != 0x2002
                && !(s[0] == 0x2001 && (s[1] < 0x200 || s[1] == 0xdb8))
                && s[0] != 0x3fff
        }
    }
}

pub fn public_url(raw: &str) -> Result<Url, Error> {
    if raw.len() > 2048 {
        return Err(error("invalid_url", "The source URL exceeds 2,048 bytes."));
    }
    let url = Url::parse(raw).map_err(|_| error("invalid_url", "Use an absolute HTTPS URL."))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
        || url.fragment().is_some()
    {
        return Err(error(
            "invalid_url",
            "Use HTTPS on port 443 without credentials or fragments.",
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| error("invalid_url", "A public hostname is required."))?;
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        if !public_ip(ip) {
            return Err(error(
                "restricted_destination",
                "Local and special-use addresses are excluded.",
            ));
        }
    } else if !host.contains('.')
        || [
            ".localhost",
            ".local",
            ".internal",
            ".home",
            ".lan",
            ".test",
            ".invalid",
            ".example",
            ".onion",
        ]
        .iter()
        .any(|suffix| host.ends_with(suffix))
    {
        return Err(error(
            "restricted_destination",
            "A public internet hostname is required.",
        ));
    }
    Ok(url)
}

#[derive(Debug)]
struct DnsRequest {
    host: String,
    reply: SyncSender<Result<Vec<SocketAddr>, Error>>,
}

/// One DNS worker and a one-slot queue bound even a stalled OS resolver. A caller times out
/// after two seconds; later calls cannot create additional resolver threads.
#[derive(Debug)]
pub struct HttpTransport {
    dns: SyncSender<DnsRequest>,
    /// Reservations survive cancellation of the application's staged result.
    next_request: BTreeMap<String, Instant>,
}

impl HttpTransport {
    pub fn new() -> Result<Self, Error> {
        let (send, receive) = mpsc::sync_channel::<DnsRequest>(1);
        std::thread::Builder::new()
            .name("oia-source-dns".into())
            .spawn(move || {
                while let Ok(request) = receive.recv() {
                    let result = (request.host.as_str(), 443)
                        .to_socket_addrs()
                        .map(|addresses| addresses.take(33).collect::<Vec<_>>())
                        .map_err(|_| {
                            error("unavailable", "The source hostname could not be resolved.")
                        });
                    let _ = request.reply.send(result);
                }
            })
            .map_err(|_| error("unavailable", "The bounded DNS worker could not start."))?;
        Ok(Self {
            dns: send,
            next_request: BTreeMap::new(),
        })
    }
}

impl Transport for HttpTransport {
    fn get(&mut self, request: Request<'_>, check: Check<'_>) -> Result<Response, Error> {
        self.get_with_policy(request, HttpPolicy::default(), check)
    }
}

impl HttpTransport {
    pub fn get_with_policy(
        &mut self,
        request: Request<'_>,
        policy: HttpPolicy,
        check: Check<'_>,
    ) -> Result<Response, Error> {
        let started = Instant::now();
        if policy.max_bytes == 0
            || policy.max_bytes > 4 * 1024 * 1024
            || policy.deadline.is_zero()
            || policy.deadline > Duration::from_secs(20)
            || policy.endpoint_budget == 0
            || policy.endpoint_budget > 2012
        {
            return Err(error(
                "invalid_budget",
                "HTTP policy exceeds the shared transport ceilings.",
            ));
        }
        let deadline = started + policy.deadline;
        check()?;
        let cadence_seconds = request.cadence_seconds;
        let url = public_url(request.url)?;
        let key = format!("{}{}", url.origin().ascii_serialization(), url.path());
        if self
            .next_request
            .get(&key)
            .is_some_and(|next| Instant::now() < *next)
        {
            return Err(error(
                "rate_limited",
                "This source's request interval is still in effect, including after cancellation.",
            ));
        }
        if self.next_request.len() >= policy.endpoint_budget
            && !self.next_request.contains_key(&key)
        {
            return Err(error(
                "source_limit",
                "The session transport endpoint budget is exhausted.",
            ));
        }
        let cadence = Duration::from_secs(cadence_seconds.clamp(10, 86400));
        self.next_request
            .insert(key.clone(), Instant::now() + cadence);
        let host = url
            .host_str()
            .ok_or_else(|| error("invalid_url", "Missing source host."))?;
        let (send, receive) = mpsc::sync_channel(1);
        self.dns
            .try_send(DnsRequest {
                host: host.to_owned(),
                reply: send,
            })
            .map_err(|_| error("unavailable", "The DNS worker is busy; retry later."))?;
        let addresses = receive
            .recv_timeout(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_secs(2)),
            )
            .map_err(|_| error("deadline", "Source DNS exceeded its bounded deadline."))??;
        check()?;
        if addresses.is_empty()
            || addresses.len() > 32
            || addresses.iter().any(|a| !public_ip(a.ip()))
        {
            return Err(error(
                "restricted_destination",
                "DNS returned a non-public address or exceeded its address budget.",
            ));
        }
        // One async transfer is bounded by the original monotonic deadline. Neither
        // delayed headers nor body chunks can restart the timer. DNS time is included.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| error("unavailable", "The HTTP runtime could not start."))?;
        let response = runtime.block_on(async {
            let client = Client::builder()
                .https_only(true)
                .no_proxy()
                .retry(reqwest::retry::never())
                .redirect(Policy::none())
                .resolve_to_addrs(host, &addresses)
                .connect_timeout(policy.connect_timeout.min(policy.deadline))
                .user_agent("OIA/0.1 public-source-reader (+https://github.com/mESmaC/oia)")
                .build()
                .map_err(|_| error("unavailable", "The HTTPS client could not start."))?;
            transfer(client, request, policy, deadline, check).await
        })?;
        if response.retry_after.is_some()
            || response.status == 429
            || response.status == 503
            || response.cache_max_age.is_some()
        {
            let delay = if response.status == 429 || response.status == 503 {
                response.retry_after.unwrap_or(86400)
            } else {
                response.retry_after.unwrap_or(0)
            }
            .max(cadence_seconds)
            .max(response.cache_max_age.unwrap_or(0));
            let next = Instant::now()
                .checked_add(Duration::from_secs(delay))
                .unwrap_or_else(|| Instant::now() + Duration::from_secs(3153600000));
            self.next_request.insert(key, next);
        }
        Ok(response)
    }
}

async fn transfer(
    client: Client,
    request: Request<'_>,
    policy: HttpPolicy,
    deadline: Instant,
    check: Check<'_>,
) -> Result<Response, Error> {
    let ensure_deadline = || {
        if Instant::now() >= deadline {
            Err(error(
                "deadline",
                "The source exceeded its whole-request deadline; no partial response was accepted.",
            ))
        } else {
            Ok(())
        }
    };
    ensure_deadline()?;
    let operation = async {
        let mut builder = client
            .get(request.url)
            .header("Accept", policy.accept)
            .header("Accept-Encoding", "identity");
        if policy.no_cache {
            builder = builder.header("Cache-Control", "no-cache");
        }
        if let Some(etag) = request.etag {
            builder = builder.header("If-None-Match", etag);
        }
        if let Some(modified) = request.last_modified {
            builder = builder.header("If-Modified-Since", modified);
        }
        let mut response = builder.send().await.map_err(|_| {
            error(
                "unavailable",
                "The source request failed; no substitute was used.",
            )
        })?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .filter(|v| v.len() <= 1024)
                .map(str::to_owned)
        };
        let status = response.status().as_u16();
        let retry_after = header("retry-after").map(|value| {
            value
                .parse::<u64>()
                .ok()
                .or_else(|| {
                    httpdate::parse_http_date(&value).ok().map(|time| {
                        time.duration_since(SystemTime::now())
                            .map(|delay| delay.as_secs().saturating_add(1))
                            .unwrap_or(0)
                    })
                })
                .unwrap_or(86400)
        });
        let cache_control = header("cache-control")
            .unwrap_or_default()
            .to_ascii_lowercase();
        let cache_max_age = cache_control.split(',').find_map(|part| {
            part.trim()
                .strip_prefix("max-age=")
                .and_then(|value| value.trim_matches('"').parse::<u64>().ok())
        });
        let no_store = cache_control
            .split(',')
            .any(|part| part.trim() == "no-store");
        let etag = header("etag");
        let last_modified = header("last-modified");
        let content_type = header("content-type");
        let mut body = Vec::new();
        // Return refusal statuses without reading their bodies or hiding them behind
        // cancellation. Adapters must latch observed restrictions even after cancel.
        if status == 200 && !no_store {
            check()?;
            if policy.expected_content_type.is_some_and(|expected| {
                content_type
                    .as_deref()
                    .and_then(|value| value.split(';').next())
                    .map(str::trim)
                    != Some(expected)
            }) {
                return Err(error(
                    "media_type",
                    "The source returned an unexpected media type.",
                ));
            }
            if header("content-encoding")
                .is_some_and(|value| !value.eq_ignore_ascii_case("identity"))
            {
                return Err(error(
                    "malformed",
                    "Compressed payloads are unsupported; the source ignored identity encoding.",
                ));
            }
            if response
                .content_length()
                .is_some_and(|n| n > policy.max_bytes as u64)
            {
                return Err(error(
                    "byte_limit",
                    "The source response exceeds its byte budget.",
                ));
            }
            loop {
                ensure_deadline()?;
                check()?;
                let Some(chunk) = response
                    .chunk()
                    .await
                    .map_err(|_| error("unavailable", "The source response was interrupted."))?
                else {
                    break;
                };
                if chunk.len() > policy.max_bytes.saturating_sub(body.len()) {
                    return Err(error(
                        "byte_limit",
                        "The source response exceeds its byte budget; no truncated payload was accepted.",
                    ));
                }
                body.extend_from_slice(&chunk);
            }
        }
        if status == 200 {
            ensure_deadline()?;
            check()?;
        }
        Ok(Response {
            status,
            body,
            etag,
            last_modified,
            retry_after,
            cache_max_age,
            no_store,
        })
    };
    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), operation)
        .await
        .map_err(|_| error("deadline", "The source exceeded its whole-request deadline (DNS, connect, headers and body); no partial response was accepted."))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    // Exercise the real transfer against a local HTTP fixture (production admits HTTPS
    // and public destinations only). No camera/provider request or TLS bypass in production.
    #[test]
    fn one_deadline_covers_delayed_headers_and_trickling_body()
    -> Result<(), Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        for trickle in [false, true] {
            let listener = TcpListener::bind(("127.0.0.1", 0))?;
            let url = format!("http://{}/fixture", listener.local_addr()?);
            let server = std::thread::spawn(move || -> std::io::Result<()> {
                let (mut stream, _) = listener.accept()?;
                stream.set_read_timeout(Some(Duration::from_secs(1)))?;
                let mut request = [0; 4096];
                let _ = stream.read(&mut request)?;
                if !trickle {
                    std::thread::sleep(Duration::from_millis(300));
                }
                if stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\n",
                    )
                    .is_err()
                {
                    return Ok(());
                }
                for _ in 0..20 {
                    if stream.write_all(b"x").is_err() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(40));
                }
                Ok(())
            });
            let started = Instant::now();
            let result = runtime.block_on(async {
                let client = Client::builder()
                    .no_proxy()
                    .retry(reqwest::retry::never())
                    .redirect(Policy::none())
                    .build()
                    .map_err(|_| error("fixture", "client"))?;
                transfer(
                    client,
                    Request {
                        url: &url,
                        cadence_seconds: 15,
                        etag: None,
                        last_modified: None,
                    },
                    HttpPolicy::default(),
                    started + Duration::from_millis(150),
                    &|| Ok(()),
                )
                .await
            });
            let elapsed = started.elapsed();
            assert_eq!(
                result.err().map(|e| e.code),
                Some("deadline"),
                "trickle={trickle}"
            );
            assert!(
                elapsed < Duration::from_millis(275),
                "transfer exceeded its single deadline: {elapsed:?}"
            );
            server.join().map_err(|_| "fixture thread panicked")??;
        }
        Ok(())
    }
}
