//! E5 — the network edge. **NETWORK-UNVERIFIED** (see `docs/SMOKE-TEST.md`).
//!
//! The socket in this file has never carried a byte in this environment: no
//! build in this stage has an endpoint baked in ([`super::config`]), and live
//! verification against the real Worker is E6/E7's gate. What IS verified here
//! is everything with a wrong answer — [`classify`], the `Retry-After` parser,
//! the URL and header construction — because those are the parts whose mistakes
//! stay invisible until a deploy starts 4xx-ing the fleet.
//!
//! The file is deliberately small: build a request, POST it, turn the status
//! into a decision. Everything interesting — whether to send at all, what to
//! send, when to retry — has already happened before [`TelemetrySender::send`]
//! is called.
//!
//! ## The status contract
//!
//! | Status | Means | This client |
//! |---|---|---|
//! | 2xx | stored | drop from the outbox (success) |
//! | 400 | schema violation | **drop, do not retry** (reason logged locally) |
//! | 401 / 403 | key rejected | **drop, do not retry** |
//! | 413 | over the size cap | **drop, do not retry** |
//! | 408 / 429 | timeout / rate limit | back off, honouring `Retry-After` |
//! | other 4xx | unrecognised refusal | **drop, do not retry** |
//! | 5xx | the endpoint's fault | back off and retry |
//! | transport error | the network's fault | back off and retry |
//!
//! 429 is the interesting entry: the only 4xx that IS transient, and the one a
//! blanket "4xx means stop" rule would get wrong by throwing away a payload the
//! endpoint explicitly asked us to send again later.
//!
//! ## What is NOT here
//!
//! No retry loop, no timer, no consent check. `send` performs exactly one
//! request and returns. The loop is [`super::sender::pump_once`], the schedule
//! is the outbox's `next_attempt`, and the consent gate is
//! [`super::outbox::pump_decision`] — which runs before this type is even
//! consulted.

use std::time::Duration;

use reqwest::{header::HeaderMap, StatusCode};

use super::config::{TelemetryEndpoint, WRITE_KEY_HEADER};
use super::sender::{SendFailure, SendFuture, TelemetrySender};

/// A per-request cap. A telemetry POST is a couple of kilobytes to an edge
/// worker; if it has not answered in fifteen seconds it is not going to, and the
/// outbox will try again in a minute. A bare `Client::new()` has NO timeout, so
/// an endpoint that accepts the request and never answers would wedge the pump
/// forever.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Posts payloads to the configured endpoint.
pub struct HttpTelemetrySender {
    client: reqwest::Client,
    endpoint: TelemetryEndpoint,
}

impl HttpTelemetrySender {
    /// Build a sender for `endpoint`. Returns `None` if a client cannot be
    /// constructed — a TLS backend that will not initialise is a build with no
    /// sender, not a build that panics on a Sunday.
    pub fn new(endpoint: TelemetryEndpoint) -> Option<Self> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .ok()?;
        Some(Self { client, endpoint })
    }

    /// The endpoint this sender was built for.
    pub fn endpoint(&self) -> &TelemetryEndpoint {
        &self.endpoint
    }
}

impl TelemetrySender for HttpTelemetrySender {
    fn send<'a>(&'a self, payload_json: &'a str) -> SendFuture<'a> {
        Box::pin(async move {
            let res = self
                .client
                .post(&self.endpoint.ingest_url)
                .header(WRITE_KEY_HEADER, &self.endpoint.write_key)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(payload_json.to_string())
                .send()
                .await;

            match res {
                Ok(r) => classify(r.status(), retry_after_ms(r.headers())),
                Err(e) => Err(SendFailure::transient(transport_error(&e))),
            }
        })
    }

    /// Ask the endpoint to delete every row for a retired install id.
    ///
    /// The remote half of "delete my data". `Ok(())` means the endpoint
    /// confirmed — including for an id it has never seen, which it reports as a
    /// success with zero counts rather than a 404, so a machine that retired an
    /// id before ever sending anything does not retry forever.
    fn delete_install<'a>(&'a self, install_id: &'a str) -> SendFuture<'a> {
        Box::pin(async move {
            let res = self
                .client
                .delete(self.endpoint.delete_url(install_id))
                .header(WRITE_KEY_HEADER, &self.endpoint.write_key)
                .send()
                .await;

            match res {
                Ok(r) => classify(r.status(), retry_after_ms(r.headers())),
                Err(e) => Err(SendFailure::transient(transport_error(&e))),
            }
        })
    }
}

/// Turn one HTTP status into the outbox's decision. See the module table.
///
/// Pure, and separated from the request precisely so it can be tested
/// exhaustively without a socket.
pub fn classify(status: StatusCode, retry_after_ms: Option<i64>) -> Result<(), SendFailure> {
    if status.is_success() {
        return Ok(());
    }
    // The two transient 4xx, named before the blanket rule below.
    if status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::REQUEST_TIMEOUT {
        return Err(SendFailure::Transient {
            message: format!("endpoint said {status}"),
            retry_after_ms,
        });
    }
    if status.is_client_error() {
        // Every other 4xx says this payload is unacceptable. Re-sending the same
        // bytes cannot change that, so the entry is dropped rather than put on a
        // ladder that reaches 24 hours before giving up. The reason is logged by
        // `sender::pump_once` — never a silent loss.
        return Err(SendFailure::Permanent(format!(
            "endpoint rejected the payload: {status}"
        )));
    }
    // 5xx and anything unrecognised: assume it is ours to wait out. The
    // conservative direction — a payload kept too long costs a queue slot, a
    // payload dropped wrongly is gone.
    Err(SendFailure::Transient {
        message: format!("endpoint said {status}"),
        retry_after_ms,
    })
}

/// The `Retry-After` header in milliseconds, when it is a delta-seconds value.
///
/// RFC 9110 also allows an HTTP-date, which is deliberately NOT parsed: an
/// absolute date depends on the two clocks agreeing, and a machine with a wrong
/// clock would then park a payload for years. The Worker sends delta-seconds.
/// An unparseable header is simply absent, and the ladder decides — the same
/// outcome the client had before it honoured the header at all.
fn retry_after_ms(headers: &HeaderMap) -> Option<i64> {
    let raw = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    let secs: i64 = raw.trim().parse().ok()?;
    secs.checked_mul(1_000)
}

/// A short, path-free description of a transport failure.
///
/// `reqwest`'s `Display` includes the full URL, which is harmless (the URL is a
/// build constant, not user data) but noisy in a settings panel. This keeps the
/// CATEGORY, which is the part that helps.
fn transport_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "the endpoint did not answer in time".to_string()
    } else if e.is_connect() {
        "could not reach the endpoint".to_string()
    } else if e.is_request() {
        "the request could not be sent".to_string()
    } else {
        "network error".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(n: u16) -> StatusCode {
        StatusCode::from_u16(n).expect("valid status")
    }

    #[test]
    fn success_is_any_2xx() {
        // The Worker answers 202 Accepted, but 200 must work too — pinning the
        // exact code would make a benign endpoint change look like a rejection.
        for n in [200u16, 201, 202, 204] {
            assert!(classify(code(n), None).is_ok(), "{n} should be a success");
        }
    }

    #[test]
    fn a_schema_rejection_is_permanent() {
        // THE case this classification exists for. A 400 six times over 24 hours
        // is six identical rejections of the same bytes.
        let e = classify(code(400), None).unwrap_err();
        assert!(matches!(e, SendFailure::Permanent(_)), "got {e:?}");
    }

    #[test]
    fn auth_and_size_failures_are_permanent_too() {
        for n in [401u16, 403, 404, 405, 413, 422] {
            let e = classify(code(n), None).unwrap_err();
            assert!(
                matches!(e, SendFailure::Permanent(_)),
                "{n} should be permanent, got {e:?}"
            );
        }
    }

    #[test]
    fn rate_limiting_and_request_timeout_are_the_transient_4xx() {
        // A blanket "4xx means stop" would throw away a payload the endpoint
        // explicitly asked us to send again later.
        for n in [408u16, 429] {
            let e = classify(code(n), None).unwrap_err();
            assert!(
                matches!(e, SendFailure::Transient { .. }),
                "{n} should be transient, got {e:?}"
            );
        }
    }

    #[test]
    fn server_errors_and_unknown_statuses_are_kept_rather_than_dropped() {
        // A payload kept too long costs a queue slot; a payload dropped wrongly
        // is gone. Unknown non-4xx errs toward keeping.
        for n in [500u16, 502, 503, 504, 599] {
            let e = classify(code(n), None).unwrap_err();
            assert!(
                matches!(e, SendFailure::Transient { .. }),
                "{n} should be transient, got {e:?}"
            );
        }
    }

    #[test]
    fn the_whole_classification_table_in_one_place() {
        // The table from the module docs, executable. A change to any row has to
        // be a change here too.
        let cases: &[(u16, &str)] = &[
            (200, "ok"),
            (202, "ok"),
            (400, "permanent"),
            (401, "permanent"),
            (403, "permanent"),
            (413, "permanent"),
            (418, "permanent"),
            (408, "transient"),
            (429, "transient"),
            (500, "transient"),
            (503, "transient"),
        ];
        for (n, expected) in cases {
            let got = match classify(code(*n), None) {
                Ok(()) => "ok",
                Err(SendFailure::Permanent(_)) => "permanent",
                Err(SendFailure::Transient { .. }) => "transient",
            };
            assert_eq!(got, *expected, "status {n}");
        }
    }

    #[test]
    fn a_retry_after_rides_along_with_a_transient_failure() {
        let e = classify(code(429), Some(90_000)).unwrap_err();
        match e {
            SendFailure::Transient { retry_after_ms, .. } => {
                assert_eq!(retry_after_ms, Some(90_000))
            }
            other => panic!("expected transient, got {other:?}"),
        }
        // …and a permanent rejection carries none, because there is no "later".
        assert!(matches!(
            classify(code(400), Some(90_000)).unwrap_err(),
            SendFailure::Permanent(_)
        ));
    }

    #[test]
    fn retry_after_parses_delta_seconds_and_refuses_everything_else() {
        let mut h = HeaderMap::new();
        h.insert(reqwest::header::RETRY_AFTER, "120".parse().expect("header"));
        assert_eq!(retry_after_ms(&h), Some(120_000));

        h.insert(reqwest::header::RETRY_AFTER, " 5 ".parse().expect("header"));
        assert_eq!(retry_after_ms(&h), Some(5_000));

        // An HTTP-date is deliberately not honoured: it depends on two clocks
        // agreeing, and a machine with a wrong clock would park a payload for
        // years. Absent means "the ladder decides", which is always safe.
        h.insert(
            reqwest::header::RETRY_AFTER,
            "Wed, 21 Oct 2026 07:28:00 GMT".parse().expect("header"),
        );
        assert_eq!(retry_after_ms(&h), None);

        h.insert(reqwest::header::RETRY_AFTER, "".parse().expect("header"));
        assert_eq!(retry_after_ms(&h), None);
        // No header at all.
        assert_eq!(retry_after_ms(&HeaderMap::new()), None);
    }

    #[test]
    fn an_absurd_retry_after_cannot_overflow_the_schedule() {
        let mut h = HeaderMap::new();
        h.insert(
            reqwest::header::RETRY_AFTER,
            i64::MAX.to_string().parse().expect("header"),
        );
        // `i64::MAX * 1000` would wrap; the multiplication is checked, so the
        // header is simply not honoured (and the outbox clamps besides).
        assert_eq!(retry_after_ms(&h), None);
    }

    #[test]
    fn the_message_never_carries_a_url() {
        // The last error is shown in the settings panel. It should say what went
        // wrong, not print an endpoint at an operator.
        for n in [400u16, 429, 500] {
            let msg = format!("{:?}", classify(code(n), None).unwrap_err());
            assert!(!msg.contains("http"), "{msg}");
        }
    }

    #[test]
    fn a_sender_can_be_built_for_a_valid_endpoint() {
        // Construction only — no request is made. Proves the client builder and
        // the URL construction agree before E6 points this at a real Worker.
        let endpoint = TelemetryEndpoint::normalize(
            Some("https://telemetry.invalid".into()),
            Some("k".into()),
        )
        .expect("endpoint");
        let sender = HttpTelemetrySender::new(endpoint).expect("client builds");
        assert_eq!(
            sender.endpoint().ingest_url,
            "https://telemetry.invalid/v1/apps/sundaystage/ingest"
        );
    }
}
