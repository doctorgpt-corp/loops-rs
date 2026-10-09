//! The high-level [`LoopsClient`].
//!
//! Requests are dispatched here rather than through the generated
//! `LoopsOpenApiSpecClient`, which discards the status code and headers of any
//! response its spec does not list. The client needs both to report rate
//! limits, so it sends the request itself and hands successful responses to
//! the generated `parse_response` functions for typed decoding.

mod campaigns;
mod contacts;
mod email_messages;
mod events;
mod groups;
mod params;
mod segments;
mod templates;
mod transactional;
mod workflow_nodes;
mod workflows;

use std::fmt;
use std::future::Future;
use std::sync::{Arc, PoisonError, RwLock};

use bytes::Bytes;
use http::{HeaderMap, HeaderValue, StatusCode};
use reqwest::Url;
use serde_json::Value;

use crate::config::get_config;
use crate::error::{LoopsError, Result};
use crate::generated::{LoopsOpenApiSpecClient, TestApiKeyRequest, TestApiKeyResponse};
use crate::routes::Route;

pub use params::{
    ContactFields, ContactId, DeleteNodeOptions, EmailFormat, EmailMessageUpdate, EventOptions,
    NodePlacement, Page, PreviewOptions, TransactionalOptions,
};

const USER_AGENT: &str = concat!("loops-so-rust/", env!("CARGO_PKG_VERSION"));

/// High-level client for the Loops API.
///
/// Wraps every Loops endpoint with typed arguments, maps failures to
/// [`LoopsError`], and enforces safe mode. Cloning is cheap: clones share the
/// underlying connection pool.
///
/// ```no_run
/// # async fn run() -> loops_so::Result<()> {
/// use loops_so::{ContactFields, ContactId, LoopsClient};
///
/// let client = LoopsClient::builder().api_key("your_api_key").build()?;
/// client
///     .upsert_contact(
///         ContactId::email("user@example.com"),
///         ContactFields::new().first_name("John"),
///     )
///     .await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct LoopsClient {
    http: reqwest::Client,
    base_url: Url,
    authorization: HeaderValue,
    safe_mode: bool,
    safe_mode_allowed_domains: Vec<String>,
}

impl fmt::Debug for LoopsClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoopsClient")
            .field("base_url", &self.base_url.as_str())
            .field("safe_mode", &self.safe_mode)
            .field("safe_mode_allowed_domains", &self.safe_mode_allowed_domains)
            .finish_non_exhaustive()
    }
}

/// Builds a [`LoopsClient`]. Every setting left unset falls back to the
/// process-wide defaults from [`configure`](crate::configure).
#[derive(Debug, Default)]
#[must_use]
pub struct LoopsClientBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    safe_mode: Option<bool>,
    safe_mode_allowed_domains: Option<Vec<String>>,
    http_client: Option<reqwest::Client>,
}

impl LoopsClientBuilder {
    /// The Loops API key. Defaults to the configured key, then the
    /// `LOOPS_API_KEY` environment variable.
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// The base URL of the Loops API (default `https://app.loops.so/api`).
    /// Do not include the `/v1` version segment; a trailing `/v1` is stripped.
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// When `true`, every recipient address must end in one of the
    /// [allowed domains](Self::safe_mode_allowed_domains) or the call fails
    /// with [`LoopsError::UnsafeEmail`] before anything is sent.
    pub fn safe_mode(mut self, safe_mode: bool) -> Self {
        self.safe_mode = Some(safe_mode);
        self
    }

    /// The domains allowed in safe mode, each starting with `@`
    /// (e.g. `["@test.com", "@example.com"]`).
    pub fn safe_mode_allowed_domains<I, S>(mut self, domains: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.safe_mode_allowed_domains = Some(domains.into_iter().map(Into::into).collect());
        self
    }

    /// Send requests through this `reqwest` client, e.g. to set timeouts or a
    /// proxy. The API key is added per request, so the client needs no
    /// default headers.
    pub fn http_client(mut self, client: reqwest::Client) -> Self {
        self.http_client = Some(client);
        self
    }

    /// Build the client.
    ///
    /// # Errors
    ///
    /// [`LoopsError::Configuration`] when no API key is available or the base
    /// URL does not parse.
    pub fn build(self) -> Result<LoopsClient> {
        let config = get_config();
        let api_key = self
            .api_key
            .filter(|key| !key.is_empty())
            .or(config.api_key)
            .ok_or_else(|| {
                LoopsError::Configuration(
                    "API key not configured. Set the LOOPS_API_KEY environment variable or call \
                 loops_so::configure(ConfigureOptions::new().api_key(\"...\"))"
                        .to_owned(),
                )
            })?;
        let base_url = self.base_url.unwrap_or(config.base_url);
        let normalized = normalize_base_url(&base_url);
        let base_url = Url::parse(&normalized)
            .ok()
            .filter(|url| !url.cannot_be_a_base())
            .ok_or_else(|| LoopsError::Configuration(format!("Invalid base_url: {base_url:?}")))?;
        let mut authorization =
            HeaderValue::try_from(format!("Bearer {api_key}")).map_err(|_| {
                LoopsError::Configuration(
                    "API key contains characters not allowed in an HTTP header".to_owned(),
                )
            })?;
        authorization.set_sensitive(true);
        let http = match self.http_client {
            Some(client) => client,
            None => reqwest::Client::builder().user_agent(USER_AGENT).build()?,
        };
        Ok(LoopsClient {
            http,
            base_url,
            authorization,
            safe_mode: self.safe_mode.unwrap_or(config.safe_mode),
            safe_mode_allowed_domains: self
                .safe_mode_allowed_domains
                .unwrap_or(config.safe_mode_allowed_domains),
        })
    }
}

/// Strip trailing slashes and a trailing `/v1` from a base URL. The version
/// segment is part of every endpoint path, so leaving it on the base URL
/// would produce `/v1/v1/...` paths.
pub(crate) fn normalize_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_owned()
}

impl LoopsClient {
    /// A client built entirely from the process-wide defaults.
    ///
    /// # Errors
    ///
    /// See [`LoopsClientBuilder::build`].
    pub fn new() -> Result<Self> {
        Self::builder().build()
    }

    /// Start building a client.
    pub fn builder() -> LoopsClientBuilder {
        LoopsClientBuilder::default()
    }

    /// The normalized base URL requests are sent to.
    pub fn base_url(&self) -> &str {
        self.base_url.as_str()
    }

    /// Whether safe mode is on for this client.
    pub fn safe_mode(&self) -> bool {
        self.safe_mode
    }

    /// A low-level generated client that shares this client's base URL and
    /// API key. Use it for endpoints or options the high-level methods do not
    /// expose. Each call builds a new connection pool, so keep the result
    /// rather than calling this per request.
    ///
    /// # Errors
    ///
    /// [`LoopsError::Http`] when the underlying `reqwest` client cannot be built.
    pub fn low_level(&self) -> Result<LoopsOpenApiSpecClient> {
        let mut headers = HeaderMap::new();
        headers.insert(http::header::AUTHORIZATION, self.authorization.clone());
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .build()?;
        Ok(LoopsOpenApiSpecClient {
            client: http,
            base_url: self.base_url.clone(),
        })
    }

    /// Validate the API key.
    ///
    /// # Errors
    ///
    /// [`LoopsError::Api`] with status 401 when the key is invalid.
    pub async fn health(&self) -> Result<bool> {
        const ACTION: &str = "validate API key";
        let raw = self.send(Route::Health, &[], |request| request).await?;
        if raw.status == StatusCode::UNAUTHORIZED {
            return Err(LoopsError::Api {
                message: "Invalid API key".to_owned(),
                status: Some(401),
                body: raw.json(),
            });
        }
        match raw.parse(ACTION, TestApiKeyRequest::parse_response).await? {
            TestApiKeyResponse::Ok(_) => Ok(true),
            _ => Err(raw.unexpected(ACTION)),
        }
    }

    /// Fail when safe mode is on and `email` is outside the allowed domains.
    fn validate_email(&self, email: Option<&str>) -> Result<()> {
        let Some(email) = email.filter(|email| !email.is_empty()) else {
            return Ok(());
        };
        if !self.safe_mode {
            return Ok(());
        }
        if self.safe_mode_allowed_domains.is_empty() {
            return Err(LoopsError::Configuration(
                "safe_mode is enabled but no safe_mode_allowed_domains configured. Set allowed \
                 domains via configure(ConfigureOptions::new().safe_mode_allowed_domains([\"@test.com\"])) \
                 or on LoopsClient::builder()."
                    .to_owned(),
            ));
        }
        let email_lower = email.to_lowercase();
        if self
            .safe_mode_allowed_domains
            .iter()
            .any(|domain| email_lower.ends_with(&domain.to_lowercase()))
        {
            Ok(())
        } else {
            Err(LoopsError::UnsafeEmail {
                email: email.to_owned(),
                allowed_domains: self.safe_mode_allowed_domains.clone(),
            })
        }
    }

    fn url(&self, route: Route, params: &[&str]) -> Url {
        let mut url = self.base_url.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .expect("base URL was checked to be a base when the client was built");
            segments.pop_if_empty();
            let mut params = params.iter();
            for segment in route
                .path()
                .split('/')
                .filter(|segment| !segment.is_empty())
            {
                if segment.starts_with('{') {
                    let value = params
                        .next()
                        .expect("every path placeholder has a parameter");
                    segments.push(value);
                } else {
                    segments.push(segment);
                }
            }
            debug_assert!(
                params.next().is_none(),
                "more parameters than placeholders for {route:?}"
            );
        }
        url
    }

    /// Send a request to `route`, buffer the response, and turn HTTP 429 into
    /// [`LoopsError::RateLimit`].
    async fn send(
        &self,
        route: Route,
        params: &[&str],
        build: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
    ) -> Result<RawResponse> {
        // An empty, `.` or `..` segment would silently address a different
        // endpoint: `url` drops dot segments, so `update_campaign_group(".")`
        // would POST to the collection and create a group.
        if let Some(param) = params
            .iter()
            .find(|param| matches!(**param, "" | "." | ".."))
        {
            return Err(LoopsError::InvalidInput(format!(
                "{} cannot take {param:?} as a path parameter",
                route.name()
            )));
        }
        let request = self
            .http
            .request(route.method().to_http(), self.url(route, params))
            .header(http::header::AUTHORIZATION, self.authorization.clone());
        let response = build(request).send().await?;
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.bytes().await?;
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(LoopsError::RateLimit {
                limit: header_number(&headers, "x-ratelimit-limit"),
                remaining: header_number(&headers, "x-ratelimit-remaining"),
            });
        }
        Ok(RawResponse {
            status,
            headers,
            body,
        })
    }
}

fn header_number(headers: &HeaderMap, name: &str) -> u64 {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

/// Merge caller-supplied properties into a JSON body as top-level keys,
/// refusing keys the method sets itself. Overwriting `email` would otherwise
/// redirect the request past the safe-mode check.
fn merge_properties(
    body: &mut serde_json::Map<String, Value>,
    properties: serde_json::Map<String, Value>,
    reserved: &[&str],
    action: &str,
) -> Result<()> {
    if let Some(key) = properties
        .keys()
        .find(|key| reserved.contains(&key.as_str()) || body.contains_key(*key))
    {
        return Err(LoopsError::InvalidInput(format!(
            "{action}: property `{key}` is a field the request sets itself; use its dedicated option"
        )));
    }
    body.extend(properties);
    Ok(())
}

/// Endpoints that look a contact up by exactly one identifier.
fn single_identifier(contact: &ContactId, action: &str) -> Result<()> {
    if matches!(contact, ContactId::Both { .. }) {
        return Err(LoopsError::InvalidInput(format!(
            "{action}: provide either email or user_id, not both"
        )));
    }
    Ok(())
}

/// Turn a generated request's `validator` failure into [`LoopsError::InvalidInput`].
fn check(value: &impl validator::Validate) -> Result<()> {
    value
        .validate()
        .map_err(|err| LoopsError::InvalidInput(err.to_string()))
}

/// Mailing list subscriptions as the JSON object Loops expects, or `None`
/// when there are none.
pub(super) fn mailing_lists_value(
    lists: &std::collections::BTreeMap<String, bool>,
) -> Option<Value> {
    (!lists.is_empty()).then(|| {
        Value::Object(
            lists
                .iter()
                .map(|(id, subscribed)| (id.clone(), Value::Bool(*subscribed)))
                .collect(),
        )
    })
}

/// A fully buffered response.
struct RawResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl RawResponse {
    fn is_success(&self) -> bool {
        self.status.is_success()
    }

    fn json(&self) -> Option<Value> {
        serde_json::from_slice(&self.body).ok()
    }

    /// The `message` field of a JSON failure body.
    fn message(&self) -> Option<String> {
        self.json()?.get("message")?.as_str().map(str::to_owned)
    }

    fn to_reqwest(&self) -> reqwest::Response {
        let mut response = http::Response::new(self.body.clone());
        *response.status_mut() = self.status;
        *response.headers_mut() = self.headers.clone();
        reqwest::Response::from(response)
    }

    /// `"Failed to {action}: {message}"`, carrying the status and body.
    fn failure(&self, action: &str) -> LoopsError {
        self.failure_with(&format!("Failed to {action}"))
    }

    /// `"{prefix}: {message}"`, or just `prefix` when the body has no message.
    fn failure_with(&self, prefix: &str) -> LoopsError {
        let message = match self.message() {
            Some(message) => format!("{prefix}: {message}"),
            None => prefix.to_owned(),
        };
        LoopsError::Api {
            message,
            status: Some(self.status.as_u16()),
            body: self.json(),
        }
    }

    /// A success status whose variant the method does not handle.
    fn unexpected(&self, action: &str) -> LoopsError {
        LoopsError::Api {
            message: format!("Failed to {action}"),
            status: Some(self.status.as_u16()),
            body: self.json(),
        }
    }

    /// Decode a successful response with a generated `parse_response`, or
    /// fail with the response's message when the status is not 2xx.
    async fn parse<T, F, Fut>(&self, action: &'static str, parse_response: F) -> Result<T>
    where
        F: FnOnce(reqwest::Response) -> Fut,
        Fut: Future<Output = anyhow::Result<T>>,
    {
        if !self.is_success() {
            return Err(self.failure(action));
        }
        parse_response(self.to_reqwest())
            .await
            .map_err(|err| self.decode_error(action, err.into()))
    }

    /// Decode the body as untyped JSON, for endpoints whose success payload is
    /// polymorphic.
    fn parse_json(&self, action: &'static str) -> Result<Value> {
        if !self.is_success() {
            return Err(self.failure(action));
        }
        serde_json::from_slice(&self.body).map_err(|err| self.decode_error(action, Box::new(err)))
    }

    /// Decode the body into a hand-written response type.
    fn parse_as<T: serde::de::DeserializeOwned>(&self, action: &'static str) -> Result<T> {
        if !self.is_success() {
            return Err(self.failure(action));
        }
        serde_json::from_slice(&self.body).map_err(|err| self.decode_error(action, Box::new(err)))
    }

    fn decode_error(
        &self,
        action: &'static str,
        source: Box<dyn std::error::Error + Send + Sync>,
    ) -> LoopsError {
        LoopsError::Decode {
            action,
            status: self.status.as_u16(),
            source,
        }
    }
}

static SINGLETON: RwLock<Option<Arc<LoopsClient>>> = RwLock::new(None);

/// The process-wide client, created from the [`configure`](crate::configure)
/// defaults on first use.
///
/// # Errors
///
/// [`LoopsError::Configuration`] when no API key is configured.
pub fn get_client() -> Result<Arc<LoopsClient>> {
    if let Some(client) = SINGLETON
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
    {
        return Ok(Arc::clone(client));
    }
    let mut slot = SINGLETON.write().unwrap_or_else(PoisonError::into_inner);
    if let Some(client) = slot.as_ref() {
        return Ok(Arc::clone(client));
    }
    let client = Arc::new(LoopsClient::new()?);
    *slot = Some(Arc::clone(&client));
    Ok(client)
}

/// Drop the process-wide client so the next [`get_client`] builds a fresh
/// one. Call it after [`configure`](crate::configure), or in test teardown.
pub fn reset_client() {
    SINGLETON
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
}

/// Whether [`get_client`] currently holds a client.
#[cfg(feature = "testing")]
pub(crate) fn has_client() -> bool {
    SINGLETON
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::normalize_base_url;

    #[test]
    fn normalize_base_url_strips_trailing_slash_and_v1() {
        for (given, expected) in [
            ("https://app.loops.so/api", "https://app.loops.so/api"),
            ("https://app.loops.so/api/", "https://app.loops.so/api"),
            ("https://app.loops.so/api/v1", "https://app.loops.so/api"),
            ("https://app.loops.so/api/v1/", "https://app.loops.so/api"),
            (
                "https://proxy.internal/loops/v1",
                "https://proxy.internal/loops",
            ),
        ] {
            assert_eq!(normalize_base_url(given), expected, "{given}");
        }
    }
}
