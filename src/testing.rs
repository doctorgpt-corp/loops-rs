//! A mock Loops API for testing code that uses this crate.
//!
//! [`LoopsMock`] starts a local [`wiremock`] server that answers every Loops
//! endpoint with a successful default response, and points the process-wide
//! client at it. Your code runs against the real [`LoopsClient`] end to end,
//! but no request leaves the process.
//!
//! Enable the `testing` feature:
//!
//! ```toml
//! [dev-dependencies]
//! loops-so = { version = "*", features = ["testing"] }
//! ```
//!
//! ```no_run
//! # async fn run() -> loops_so::Result<()> {
//! use loops_so::routes::Route;
//! use loops_so::testing::LoopsMock;
//! use loops_so::TransactionalOptions;
//!
//! let mock = LoopsMock::start().await;
//! let client = loops_so::get_client()?;
//! client
//!     .send_transactional_email("tpl_abc", "user@test.com", TransactionalOptions::new())
//!     .await?;
//!
//! // Inspect what was sent over the wire.
//! let body: serde_json::Value = mock.calls(Route::Transactional).await[0].body_json().unwrap();
//! assert_eq!(body["transactionalId"], "tpl_abc");
//! # Ok(())
//! # }
//! ```
//!
//! Simulate a failure by overriding a route before the call. The latest
//! override of a route wins; [`LoopsMock::reset_route`] restores the default:
//!
//! ```no_run
//! # async fn run() {
//! use loops_so::routes::Route;
//! use loops_so::testing::{LoopsMock, wiremock::ResponseTemplate};
//!
//! let mock = LoopsMock::start().await;
//! mock.respond(
//!     Route::Transactional,
//!     ResponseTemplate::new(429)
//!         .insert_header("x-ratelimit-limit", "10")
//!         .insert_header("x-ratelimit-remaining", "0"),
//! );
//! // send_transactional_email now fails with LoopsError::RateLimit.
//! # }
//! ```
//!
//! While a `LoopsMock` is alive it holds a process-wide lock, so tests that
//! use it run one at a time even under the default parallel test runner.
//! Dropping it resets the client singleton and restores the configuration it
//! replaced. Starting a second mock while one is alive therefore waits for
//! the first to drop; in a single test, drop the first one explicitly.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, PoisonError};

use serde_json::{Value, json};
use tokio::sync::{Mutex, MutexGuard};
pub use wiremock;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use crate::LoopsClient;
use crate::config::{ConfigureOptions, configure, snapshot};
use crate::routes::Route;

/// The API key the mock configures unless told otherwise.
pub const TEST_API_KEY: &str = "test-key";

/// Path prefix the mock serves the API under, mirroring
/// `https://app.loops.so/api`.
const API_PREFIX: &str = "/api";

const ROUTE_PRIORITY: u8 = 1;
const UNMATCHED_PRIORITY: u8 = u8::MAX;

static GLOBAL: Mutex<()> = Mutex::const_new(());

/// Configures a [`LoopsMock`] before it starts.
#[derive(Debug, Clone)]
#[must_use]
pub struct LoopsMockBuilder {
    api_key: String,
    safe_mode: bool,
    safe_mode_allowed_domains: Vec<String>,
}

impl Default for LoopsMockBuilder {
    fn default() -> Self {
        Self {
            api_key: TEST_API_KEY.to_owned(),
            safe_mode: false,
            safe_mode_allowed_domains: Vec::new(),
        }
    }
}

impl LoopsMockBuilder {
    /// The API key to configure (default [`TEST_API_KEY`]).
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = api_key.into();
        self
    }

    /// Turn safe mode on. The mock defaults to safe mode off so tests can use
    /// any address.
    pub fn safe_mode(mut self, safe_mode: bool) -> Self {
        self.safe_mode = safe_mode;
        self
    }

    /// The domains allowed in safe mode, each starting with `@`.
    pub fn safe_mode_allowed_domains<I, S>(mut self, domains: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.safe_mode_allowed_domains = domains.into_iter().map(Into::into).collect();
        self
    }

    /// Start the mock server, mount the default routes and configure the
    /// process-wide client to use it.
    pub async fn start(self) -> LoopsMock {
        let guard = GLOBAL.lock().await;
        let restore = snapshot();
        let server = MockServer::start().await;
        let mut overrides = HashMap::new();
        for route in Route::ALL {
            let (status, body) = default_response(*route);
            let mut default = ResponseTemplate::new(status);
            if let Some(body) = body {
                default = default.set_body_json(body);
            }
            let slot = Arc::new(StdMutex::new(None));
            overrides.insert(*route, Arc::clone(&slot));
            Mock::given(method(route.method().as_str()))
                .and(path_regex(route_regex(*route)))
                .respond_with(RouteResponder { default, slot })
                .with_priority(ROUTE_PRIORITY)
                .named(route.name())
                .mount(&server)
                .await;
        }
        Mock::given(wiremock::matchers::any())
            .respond_with(|request: &Request| {
                ResponseTemplate::new(501).set_body_json(json!({
                    "success": false,
                    "message": format!(
                        "loops-so testing: no mock route for {} {}",
                        request.method,
                        request.url.path()
                    ),
                }))
            })
            .with_priority(UNMATCHED_PRIORITY)
            .named("unmatched")
            .mount(&server)
            .await;

        let base_url = format!("{}{API_PREFIX}", server.uri());
        configure(
            ConfigureOptions::new()
                .api_key(&self.api_key)
                .base_url(&base_url)
                .safe_mode(self.safe_mode)
                .safe_mode_allowed_domains(self.safe_mode_allowed_domains.clone()),
        );
        crate::reset_client();
        LoopsMock {
            server,
            base_url,
            settings: self,
            overrides,
            restore: Some(Box::new(restore)),
            _guard: guard,
        }
    }
}

/// A running mock of the Loops API. See the [module docs](self).
pub struct LoopsMock {
    server: MockServer,
    base_url: String,
    settings: LoopsMockBuilder,
    overrides: HashMap<Route, OverrideSlot>,
    restore: Option<Box<dyn FnOnce() + Send>>,
    _guard: MutexGuard<'static, ()>,
}

impl std::fmt::Debug for LoopsMock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoopsMock")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl LoopsMock {
    /// Start a mock with the default settings.
    pub async fn start() -> Self {
        Self::builder().start().await
    }

    /// Configure a mock before starting it.
    pub fn builder() -> LoopsMockBuilder {
        LoopsMockBuilder::default()
    }

    /// The base URL the mock serves the API under.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The underlying wiremock server, for custom matchers.
    pub fn server(&self) -> &MockServer {
        &self.server
    }

    /// A new client bound to the mock with the mock's settings, independent
    /// of the process-wide singleton.
    ///
    /// # Panics
    ///
    /// Never in practice: the mock's settings always form a valid client.
    pub fn client(&self) -> LoopsClient {
        LoopsClient::builder()
            .api_key(&self.settings.api_key)
            .base_url(&self.base_url)
            .safe_mode(self.settings.safe_mode)
            .safe_mode_allowed_domains(self.settings.safe_mode_allowed_domains.clone())
            .build()
            .expect("the mock's settings form a valid client")
    }

    /// Every request received on `route`, oldest first.
    pub async fn calls(&self, route: Route) -> Vec<Request> {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|request| {
                request.method.as_str() == route.method().as_str()
                    && matches_route(route, request.url.path())
            })
            .collect()
    }

    /// Whether `route` received at least one request.
    pub async fn called(&self, route: Route) -> bool {
        !self.calls(route).await.is_empty()
    }

    /// Answer `route` with `response` from now on. The latest override of a
    /// route wins.
    pub fn respond(&self, route: Route, response: ResponseTemplate) {
        *self.overrides[&route]
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(response);
    }

    /// Go back to the default response for `route`.
    pub fn reset_route(&self, route: Route) {
        self.overrides[&route]
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }

    /// Answer `route` with a JSON body and status from now on.
    pub fn respond_json(&self, route: Route, status: u16, body: Value) {
        self.respond(route, ResponseTemplate::new(status).set_body_json(body));
    }
}

impl Drop for LoopsMock {
    fn drop(&mut self) {
        crate::reset_client();
        if let Some(restore) = self.restore.take() {
            restore();
        }
    }
}

type OverrideSlot = Arc<StdMutex<Option<ResponseTemplate>>>;

/// Answers one route: the latest override when there is one, else the
/// default response.
struct RouteResponder {
    default: ResponseTemplate,
    slot: OverrideSlot,
}

impl Respond for RouteResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        self.slot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .unwrap_or_else(|| self.default.clone())
    }
}

/// Whether [`get_client`](crate::get_client) currently holds a client.
pub fn client_is_cached() -> bool {
    crate::client::has_client()
}

/// An anchored regex matching the route's path under [`API_PREFIX`], with
/// each `{placeholder}` matching one path segment.
fn route_regex(route: Route) -> String {
    let mut pattern = format!("^{API_PREFIX}");
    for segment in route.path().split('/').filter(|s| !s.is_empty()) {
        pattern.push('/');
        if segment.starts_with('{') {
            pattern.push_str("[^/]+");
        } else {
            pattern.push_str(&escape(segment));
        }
    }
    pattern.push('$');
    pattern
}

fn escape(segment: &str) -> String {
    segment
        .chars()
        .flat_map(|c| {
            let escaped = matches!(
                c,
                '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$' | '\\'
            );
            escaped
                .then_some('\\')
                .into_iter()
                .chain(std::iter::once(c))
        })
        .collect()
}

/// Whether `path` is a request path for `route`, the same test the mounted
/// [`route_regex`] performs.
fn matches_route(route: Route, path: &str) -> bool {
    let Some(rest) = path.strip_prefix(API_PREFIX) else {
        return false;
    };
    let expected = route.path().split('/').filter(|s| !s.is_empty());
    let actual = rest.split('/').filter(|s| !s.is_empty());
    expected.clone().count() == actual.clone().count()
        && expected
            .zip(actual)
            .all(|(segment, part)| segment.starts_with('{') || segment == part)
}

const TIMESTAMP: &str = "2024-01-01T00:00:00.000Z";

fn pagination() -> Value {
    json!({
        "totalResults": 0,
        "returnedResults": 0,
        "perPage": 20,
        "totalPages": 0,
        "nextCursor": null,
        "nextPage": null,
    })
}

fn empty_page() -> Value {
    json!({ "pagination": pagination(), "data": [] })
}

fn campaign(name: &str, updated_at: &str, email_message_id: Option<&str>) -> Value {
    json!({
        "id": "mock-campaign-id",
        "url": "https://app.loops.so/campaigns/mock-campaign-id",
        "name": name,
        "status": "Draft",
        "createdAt": TIMESTAMP,
        "updatedAt": updated_at,
        "emailMessageId": email_message_id,
        "campaignGroupId": null,
        "mailingListId": null,
        "audienceSegmentId": null,
        "audienceFilter": null,
        "scheduling": { "method": "now", "timestamp": null },
    })
}

fn component(affected: Option<u64>) -> Value {
    let mut value = json!({
        "id": "mock-component-id",
        "name": "Mock Component",
        "lmx": "<Text>Hello</Text>",
    });
    if let Some(count) = affected {
        value["affectedEmailCount"] = json!(count);
    }
    value
}

fn theme(updated_at: &str, affected: Option<u64>) -> Value {
    let mut value = json!({
        "id": "mock-theme-id",
        "name": "Mock Theme",
        "styles": {},
        "isDefault": false,
        "createdAt": TIMESTAMP,
        "updatedAt": updated_at,
    });
    if let Some(count) = affected {
        value["affectedEmailCount"] = json!(count);
    }
    value
}

fn email_message() -> Value {
    json!({
        "id": "mock-email-message-id",
        "campaignId": null,
        "subject": "Mock Subject",
        "previewText": "",
        "fromName": "Test",
        "fromEmail": "test",
        "replyToEmail": "",
        "emailFormat": "styled",
        "lmx": "<Text>Hello</Text>",
        "contentRevisionId": "rev-1",
        "updatedAt": TIMESTAMP,
    })
}

fn transactional_resource() -> Value {
    json!({
        "id": "mock-transactional-id",
        "url": "https://app.loops.so/transactional/mock-transactional-id",
        "name": "Mock Transactional",
        "draftEmailMessageId": null,
        "publishedEmailMessageId": null,
        "transactionalGroupId": null,
        "createdAt": TIMESTAMP,
        "updatedAt": TIMESTAMP,
        "dataVariables": [],
    })
}

fn transactional_draft() -> Value {
    json!({
        "id": "mock-transactional-id",
        "url": "https://app.loops.so/transactional/mock-transactional-id",
        "name": "Mock Transactional",
        "draftEmailMessageId": null,
        "draftEmailMessageContentRevisionId": null,
        "publishedEmailMessageId": null,
        "transactionalGroupId": null,
        "createdAt": TIMESTAMP,
        "updatedAt": TIMESTAMP,
        "dataVariables": [],
    })
}

fn workflow() -> Value {
    json!({
        "id": "mock-workflow-id",
        "url": "https://app.loops.so/workflows/mock-workflow-id",
        "workflowRevisionId": "mock-revision-id",
        "status": "Draft",
        "name": "Mock Workflow",
        "description": null,
        "mailingListId": null,
        "rootNodeId": "mock-node-id",
        "nodes": {},
    })
}

fn mutation_node() -> Value {
    json!({
        "id": "mock-node-id",
        "typeName": "SignupTrigger",
        "nextNodeIds": [],
        "workflowRevisionId": "mock-revision-id",
    })
}

fn node_with_workflow() -> Value {
    let mut node = mutation_node();
    node["workflow"] = workflow();
    node
}

fn deleted_nodes() -> Value {
    json!({
        "status": "deleted",
        "nodeIds": ["mock-node-id"],
        "workflowRevisionId": "mock-revision-id",
        "queuedContactCount": 0,
        "queuedContactLimitReached": false,
        "workflow": workflow(),
    })
}

fn audience_segment(description: Option<&str>) -> Value {
    json!({
        "id": "mock-segment-id",
        "name": "Mock Segment",
        "description": description,
        "createdAt": TIMESTAMP,
        "updatedAt": TIMESTAMP,
        "filter": null,
    })
}

fn group() -> Value {
    json!({
        "id": "mock-group-id",
        "name": "Mock Group",
        "description": "A mock group",
        "createdAt": TIMESTAMP,
        "updatedAt": TIMESTAMP,
    })
}

fn event_pattern() -> Value {
    json!({
        "id": "mock-event-pattern-id",
        "eventName": "signup",
        "eventProperties": [],
        "incomingWebhookPlatform": null,
    })
}

fn email_metrics() -> Value {
    json!({
        "sends": 0,
        "opens": 0,
        "clicks": 0,
        "unsubscribes": 0,
        "spamReports": 0,
        "hardBounces": 0,
        "softBounces": 0,
    })
}

/// The status and JSON body each route answers with by default.
#[allow(clippy::too_many_lines)]
fn default_response(route: Route) -> (u16, Option<Value>) {
    let ok = |body: Value| (200, Some(body));
    let created = |body: Value| (201, Some(body));
    match route {
        Route::Health => ok(json!({ "success": true, "teamName": "Test" })),
        Route::Transactional | Route::SendEvent => ok(json!({ "success": true })),
        Route::ListTransactional => ok(json!({
            "pagination": { "totalResults": 0, "returnedResults": 0, "perPage": 20, "totalPages": 0 },
            "data": [],
        })),
        Route::CreateContact | Route::UpsertContact => {
            ok(json!({ "success": true, "id": "mock-contact-id" }))
        }
        Route::FindContact => ok(json!([{
            "id": "mock-contact-id",
            "email": "user@test.com",
            "firstName": "Test",
            "lastName": "User",
            "source": "API",
            "subscribed": true,
            "userGroup": "",
            "userId": null,
            "mailingLists": {},
            "optInStatus": null,
        }])),
        Route::DeleteContact => ok(json!({ "success": true, "message": "Contact deleted." })),
        Route::ListContactProperties | Route::ListMailingLists | Route::ListSendingIps => {
            ok(json!([]))
        }
        Route::CreateContactProperty => ok(json!({
            "success": true,
            "key": "custom_prop",
            "label": "Custom Prop",
            "type": "string",
        })),
        Route::GetContactSuppression => ok(json!({
            "contact": { "id": "mock-contact-id", "email": "user@test.com", "userId": null },
            "isSuppressed": false,
            "removalQuota": { "limit": 3, "used": 0, "remaining": 3 },
        })),
        Route::RemoveContactSuppression => ok(json!({
            "success": true,
            "message": "Contact removed from suppression list.",
            "removalQuota": { "limit": 3, "used": 1, "remaining": 2 },
        })),
        Route::ListCampaigns
        | Route::ListComponents
        | Route::ListThemes
        | Route::ListTransactionalTemplates
        | Route::ListWorkflows
        | Route::ListAudienceSegments
        | Route::ListCampaignGroups
        | Route::ListTransactionalGroups
        | Route::ListEventPatterns => ok(empty_page()),
        Route::GetCampaign => ok(campaign("Mock Campaign", TIMESTAMP, None)),
        Route::CreateCampaign => {
            let mut body = campaign("Mock Campaign", TIMESTAMP, Some("mock-email-message-id"));
            body["emailMessageContentRevisionId"] = Value::Null;
            created(body)
        }
        Route::UpdateCampaign => ok(campaign(
            "Updated Campaign",
            "2024-01-02T00:00:00.000Z",
            None,
        )),
        Route::GetCampaignMetrics | Route::GetWorkflowNodeMetrics => ok(email_metrics()),
        Route::GetComponent => ok(component(None)),
        Route::CreateComponent => created(component(None)),
        Route::UpdateComponent => ok(component(Some(0))),
        Route::GetTheme => ok(theme(TIMESTAMP, None)),
        Route::CreateTheme => created(theme(TIMESTAMP, None)),
        Route::UpdateTheme => ok(theme("2024-01-02T00:00:00.000Z", Some(0))),
        Route::GetEmailMessage | Route::UpdateEmailMessage => ok(email_message()),
        Route::GetEmailMessageGuardian => ok(json!({ "errors": [], "warnings": [] })),
        Route::PreviewEmailMessage => ok(json!({ "id": "mock-preview-id" })),
        Route::GetTransactionalTemplate
        | Route::UpdateTransactionalTemplate
        | Route::PublishTransactionalTemplate => ok(transactional_resource()),
        Route::CreateTransactionalTemplate => created(transactional_draft()),
        Route::DraftTransactionalTemplate => ok(transactional_draft()),
        Route::GetTransactionalTemplateMetrics => ok(json!({
            "sends": 0,
            "deliveries": 0,
            "spamReports": 0,
            "hardBounces": 0,
            "softBounces": 0,
        })),
        Route::CreateUpload => ok(json!({
            "emailAssetId": "mock-asset-id",
            "presignedUrl": "https://example.com/upload",
        })),
        Route::CompleteUpload => ok(json!({
            "emailAssetId": "mock-asset-id",
            "finalUrl": "https://example.com/final.png",
        })),
        Route::GetWorkflow | Route::CreateWorkflow | Route::UpdateWorkflow => ok(workflow()),
        Route::GetWorkflowNode => ok(json!({
            "id": "mock-node-id",
            "workflowId": "mock-workflow-id",
            "typeName": "ExitAction",
            "nextNodeIds": [],
            "workflowRevisionId": "mock-revision-id",
        })),
        Route::ChangeWorkflowMailingList => ok(json!({
            "status": "updated",
            "mailingListId": "mock-list-id",
            "workflowRevisionId": "mock-revision-id",
            "queuedContactCount": 0,
            "queuedContactLimitReached": false,
            "workflow": workflow(),
        })),
        Route::DeleteWorkflow => (204, None),
        Route::CreateWorkflowNode | Route::AddWorkflowBranch => {
            ok(json!({ "node": mutation_node(), "workflow": workflow() }))
        }
        Route::UpdateWorkflowNode | Route::RerouteNodeConnection => ok(node_with_workflow()),
        Route::DeleteWorkflowNode | Route::DeleteWorkflowNodeRecursively => ok(deleted_nodes()),
        Route::GetAudienceSegment => ok(audience_segment(Some("A mock audience segment"))),
        Route::CreateAudienceSegment => ok(audience_segment(None)),
        Route::GetCampaignGroup
        | Route::CreateCampaignGroup
        | Route::UpdateCampaignGroup
        | Route::GetTransactionalGroup
        | Route::CreateTransactionalGroup
        | Route::UpdateTransactionalGroup => ok(group()),
        Route::GetEventPattern | Route::GetEventPatternByName => ok(event_pattern()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::get_config;

    // The only test in the library's test binary that touches the
    // process-wide configuration, so reading it outside the mock is safe.
    #[tokio::test]
    async fn drop_resets_singleton_and_restores_config() {
        configure(
            ConfigureOptions::new()
                .api_key("before-key")
                .base_url("https://before.example/api"),
        );
        {
            let mock = LoopsMock::start().await;
            assert_eq!(get_config().base_url, mock.base_url());
            configure(ConfigureOptions::new().api_key("changed-inside"));
            crate::get_client().unwrap();
            assert!(client_is_cached());
        }
        assert!(!client_is_cached());
        let config = get_config();
        assert_eq!(config.api_key.as_deref(), Some("before-key"));
        assert_eq!(config.base_url, "https://before.example/api");
    }

    #[test]
    fn route_patterns_match_their_own_paths_only() {
        for route in Route::ALL {
            let sample = format!(
                "{API_PREFIX}{}",
                route
                    .path()
                    .replace("{id}", "x1")
                    .replace("{node_id}", "n1")
                    .replace("{name}", "e1")
            );
            assert!(
                matches_route(*route, &sample),
                "{route:?} does not match {sample}"
            );
            for other in Route::ALL
                .iter()
                .filter(|o| **o != *route && o.method() == route.method())
            {
                assert!(
                    !matches_route(*other, &sample),
                    "{other:?} also matches {sample}, which belongs to {route:?}"
                );
            }
        }
    }
}
