//! Bounded, injectable HTTP transport shared by every import request.
use std::io::Read;
use std::time::{Duration, Instant};

use super::ImportError;

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
pub const OPTIONAL_TIMEOUT: Duration = Duration::from_secs(5);
const IMPORT_TIMEOUT: Duration = Duration::from_secs(90);
const METADATA_LIMIT: u64 = 4 * 1024 * 1024;

pub struct HttpResponse {
    pub body: Box<dyn Read>,
    pub content_length: Option<u64>,
}

impl HttpResponse {
    pub fn text(self) -> Result<String, ImportError> {
        if self
            .content_length
            .is_some_and(|length| length > METADATA_LIMIT)
        {
            return Err(ImportError::Parse("Metadata exceeds 4 MiB limit".into()));
        }
        let mut bytes = Vec::new();
        self.body
            .take(METADATA_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| ImportError::Network(error.to_string()))?;
        if bytes.len() as u64 > METADATA_LIMIT {
            return Err(ImportError::Parse("Metadata exceeds 4 MiB limit".into()));
        }
        String::from_utf8(bytes).map_err(|error| ImportError::Parse(error.to_string()))
    }

    pub fn json(self) -> Result<serde_json::Value, ImportError> {
        serde_json::from_str(&self.text()?).map_err(|error| ImportError::Parse(error.to_string()))
    }
}

pub trait HttpTransport: Send + Sync {
    fn get(&self, url: &str, budget: Duration) -> Result<HttpResponse, ImportError>;
}

pub struct HttpClient {
    agent: ureq::Agent,
    deadline: Instant,
}

impl HttpClient {
    pub fn new() -> Result<Self, ImportError> {
        let builder = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(Duration::from_secs(10))
            .timeout_write(Duration::from_secs(10))
            .timeout(REQUEST_TIMEOUT)
            .redirects(5)
            .user_agent(concat!(
                "bibtui/",
                env!("CARGO_PKG_VERSION"),
                " (https://github.com/jkulesza/bibtui)"
            ));
        // ureq's native-tls feature only supplies this adapter; it does not
        // select it automatically. Keep normal certificate/hostname validation.
        #[cfg(not(target_env = "musl"))]
        let builder = builder.tls_connector(std::sync::Arc::new(
            ureq::native_tls::TlsConnector::new()
                .map_err(|error| ImportError::Network(error.to_string()))?,
        ));
        // musl uses ureq's default rustls/webpki trust roots.
        Ok(Self {
            agent: builder.build(),
            deadline: Instant::now() + IMPORT_TIMEOUT,
        })
    }
}

impl HttpTransport for HttpClient {
    fn get(&self, url: &str, budget: Duration) -> Result<HttpResponse, ImportError> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        let timeout = budget.min(remaining).min(REQUEST_TIMEOUT);
        if timeout.is_zero() {
            return Err(ImportError::Network("Import time budget exhausted".into()));
        }
        let response = self
            .agent
            .get(url)
            .timeout(timeout)
            .call()
            .map_err(|error| ImportError::Network(error.to_string()))?;
        let content_length = response
            .header("Content-Length")
            .and_then(|value| value.parse().ok());
        Ok(HttpResponse {
            body: response.into_reader(),
            content_length,
        })
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    pub struct MockHttp {
        replies: Mutex<VecDeque<Result<&'static str, &'static str>>>,
        pub requests: Mutex<Vec<(String, Duration)>>,
    }
    impl MockHttp {
        pub fn new(replies: Vec<Result<&'static str, &'static str>>) -> Self {
            Self {
                replies: Mutex::new(replies.into()),
                requests: Mutex::new(vec![]),
            }
        }
    }
    impl HttpTransport for MockHttp {
        fn get(&self, url: &str, budget: Duration) -> Result<HttpResponse, ImportError> {
            self.requests.lock().unwrap().push((url.into(), budget));
            self.replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected HTTP request")
                .map(|body| HttpResponse {
                    body: Box::new(std::io::Cursor::new(body.as_bytes())),
                    content_length: None,
                })
                .map_err(|error| ImportError::Network(error.into()))
        }
    }

    #[test]
    fn exhausted_budget_is_rejected_without_network() {
        let mut client = HttpClient::new().unwrap();
        client.deadline = Instant::now();
        assert!(
            matches!(client.get("https://example.invalid", REQUEST_TIMEOUT), Err(ImportError::Network(message)) if message.contains("budget"))
        );
    }

    #[test]
    fn metadata_reads_are_bounded_with_or_without_content_length() {
        for content_length in [None, Some(METADATA_LIMIT + 1)] {
            let response = HttpResponse {
                body: Box::new(std::io::repeat(b'x')),
                content_length,
            };
            assert!(response.text().unwrap_err().to_string().contains("limit"));
        }
    }
    #[test]
    fn redirects_and_body_timeouts_use_the_injected_agent_budget() {
        use std::io::Write;
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            let mut handled = 0;
            while handled < 3 && Instant::now() < deadline {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut request = [0; 4096];
                let count = stream.read(&mut request).unwrap();
                if request[..count].starts_with(b"GET /redirect ") {
                    stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: /ok\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                } else if request[..count].starts_with(b"GET /stall ") {
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n",
                        )
                        .unwrap();
                    std::thread::sleep(Duration::from_millis(250));
                } else {
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                        )
                        .unwrap();
                }
                handled += 1;
            }
        });
        let client = HttpClient::new().unwrap();
        assert_eq!(
            client
                .get(
                    &format!("http://{address}/redirect"),
                    Duration::from_secs(1)
                )
                .unwrap()
                .text()
                .unwrap(),
            "ok"
        );
        let start = Instant::now();
        let result = client
            .get(
                &format!("http://{address}/stall"),
                Duration::from_millis(100),
            )
            .and_then(HttpResponse::text);
        assert!(matches!(result, Err(ImportError::Network(_))));
        assert!(start.elapsed() < Duration::from_secs(2));
        server.join().unwrap();
    }
}
