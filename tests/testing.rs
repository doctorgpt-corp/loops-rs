//! Every `LoopsClient` method against the `testing` mock: happy paths,
//! request bodies, error mapping, and the mock's own lifecycle.

use std::sync::Arc;

use loops_so::generated::{
    ChangeWorkflowMailingListResponse, ContactPropertyCreateRequestType,
    CreateWorkflowNodeTypeName, DeleteWorkflowNodeResponse, SimplifiedWorkflowNode, ThemeStyles,
};
use loops_so::routes::Route;
use loops_so::testing::LoopsMock;
use loops_so::testing::wiremock::ResponseTemplate;
use loops_so::{
    ConfigureOptions, ContactFields, ContactId, DeleteNodeOptions, EmailFormat, EmailMessageUpdate,
    EventOptions, LoopsClient, LoopsError, NodePlacement, Page, PreviewOptions,
    TransactionalOptions, configure, get_client, reset_client,
};
use serde_json::{Value, json};

async fn setup() -> (LoopsMock, Arc<LoopsClient>) {
    let mock = LoopsMock::start().await;
    let client = get_client().expect("the mock configures an API key");
    (mock, client)
}

/// The JSON body of the `n`th request `route` received.
async fn body(mock: &LoopsMock, route: Route) -> Value {
    mock.calls(route).await[0]
        .body_json()
        .expect("request body is JSON")
}

fn failure() -> Value {
    json!({ "success": false, "message": "Something went wrong" })
}

fn rate_limited(limit: &str) -> ResponseTemplate {
    ResponseTemplate::new(429)
        .set_body_json(json!({ "success": false }))
        .insert_header("x-ratelimit-limit", limit)
        .insert_header("x-ratelimit-remaining", "0")
}

#[track_caller]
fn assert_api_error(err: &LoopsError, needle: &str, status: u16) {
    assert!(
        matches!(err, LoopsError::Api { .. }),
        "expected an API error, got {err:?}"
    );
    assert!(
        err.to_string().contains(needle),
        "{err} does not mention {needle:?}"
    );
    assert_eq!(err.status_code(), Some(status), "{err}");
}

#[track_caller]
fn assert_rate_limit(err: &LoopsError, expected_limit: u64) {
    match err {
        LoopsError::RateLimit { limit, remaining } => {
            assert_eq!(*limit, expected_limit);
            assert_eq!(*remaining, 0);
        }
        other => panic!("expected a rate limit error, got {other:?}"),
    }
    assert_eq!(err.status_code(), Some(429));
}

// ---------------------------------------------------------------------------
// Default success responses
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health() {
    let (mock, client) = setup().await;
    assert!(client.health().await.unwrap());
    assert!(mock.called(Route::Health).await);
}

#[tokio::test]
async fn health_invalid_key() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::Health,
        401,
        json!({ "success": false, "message": "Invalid API key" }),
    );
    let err = client.health().await.unwrap_err();
    assert_api_error(&err, "Invalid API key", 401);
}

#[tokio::test]
async fn send_transactional_email() {
    let (mock, client) = setup().await;
    let result = client
        .send_transactional_email(
            "tpl_abc",
            "user@test.com",
            TransactionalOptions::new().data_variable("name", "Jan"),
        )
        .await
        .unwrap();
    assert!(result.success);

    let request = &mock.calls(Route::Transactional).await[0];
    let sent: Value = request.body_json().unwrap();
    assert_eq!(sent["transactionalId"], "tpl_abc");
    assert_eq!(sent["email"], "user@test.com");
    assert_eq!(sent["dataVariables"]["name"], "Jan");
    assert!(sent.get("attachments").is_none());
    assert!(sent.get("addToAudience").is_none());
    assert_eq!(request.headers["authorization"], "Bearer test-key");
    let key = request.headers["idempotency-key"].to_str().unwrap();
    assert!(
        uuid_like(key),
        "generated idempotency key {key:?} is not a UUID"
    );
}

#[tokio::test]
async fn send_transactional_email_explicit_idempotency_key_and_attachments() {
    let (mock, client) = setup().await;
    client
        .send_transactional_email(
            "tpl_abc",
            "user@test.com",
            TransactionalOptions::new()
                .idempotency_key("key-1")
                .add_to_audience(true)
                .attachment("a.txt", "text/plain", "aGk="),
        )
        .await
        .unwrap();
    let request = &mock.calls(Route::Transactional).await[0];
    assert_eq!(request.headers["idempotency-key"], "key-1");
    let sent: Value = request.body_json().unwrap();
    assert_eq!(sent["addToAudience"], true);
    assert_eq!(
        sent["attachments"],
        json!([{ "filename": "a.txt", "contentType": "text/plain", "data": "aGk=" }])
    );
}

#[tokio::test]
async fn send_transactional_email_rejects_long_idempotency_key() {
    let (mock, client) = setup().await;
    let err = client
        .send_transactional_email(
            "tpl_abc",
            "user@test.com",
            TransactionalOptions::new().idempotency_key("k".repeat(101)),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, LoopsError::InvalidInput(_)), "{err:?}");
    assert!(!mock.called(Route::Transactional).await);
}

#[tokio::test]
async fn create_contact() {
    let (mock, client) = setup().await;
    let result = client
        .create_contact(
            "new@test.com",
            Some("user-1"),
            ContactFields::new()
                .first_name("Ada")
                .source("signup-form")
                .mailing_list("list-1", true)
                .property("plan", "pro")
                .property("seats", 3)
                .property("trial", false),
        )
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.id, "mock-contact-id");
    assert_eq!(
        body(&mock, Route::CreateContact).await,
        json!({
            "email": "new@test.com",
            "firstName": "Ada",
            "source": "signup-form",
            "userId": "user-1",
            "mailingLists": { "list-1": true },
            "plan": "pro",
            "seats": 3,
            "trial": false,
        })
    );
}

#[tokio::test]
async fn create_contact_conflict() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::CreateContact,
        409,
        json!({ "success": false, "message": "Email or userId is already on list." }),
    );
    let err = client
        .create_contact("dup@test.com", None, ContactFields::new())
        .await
        .unwrap_err();
    match &err {
        LoopsError::ContactExists { message } => {
            assert_eq!(message, "Email or userId is already on list.");
        }
        other => panic!("expected ContactExists, got {other:?}"),
    }
    assert_eq!(err.status_code(), Some(409));
}

#[tokio::test]
async fn create_contact_400() {
    let (mock, client) = setup().await;
    mock.respond_json(Route::CreateContact, 400, failure());
    let err = client
        .create_contact("bad@test.com", None, ContactFields::new())
        .await
        .unwrap_err();
    assert_api_error(&err, "Failed to create contact: Something went wrong", 400);
}

#[tokio::test]
async fn upsert_contact() {
    let (mock, client) = setup().await;
    let result = client
        .upsert_contact(
            ContactId::email("user@test.com"),
            ContactFields::new().first_name("Updated"),
        )
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(
        body(&mock, Route::UpsertContact).await,
        json!({ "email": "user@test.com", "firstName": "Updated" })
    );
}

#[tokio::test]
async fn upsert_contact_by_both_identifiers() {
    let (mock, client) = setup().await;
    client
        .upsert_contact(
            ContactId::both("new@test.com", "user-1"),
            ContactFields::new().subscribed(false),
        )
        .await
        .unwrap();
    assert_eq!(
        body(&mock, Route::UpsertContact).await,
        json!({ "email": "new@test.com", "userId": "user-1", "subscribed": false })
    );
}

#[tokio::test]
async fn upsert_contact_400() {
    let (mock, client) = setup().await;
    mock.respond_json(Route::UpsertContact, 400, failure());
    let err = client
        .upsert_contact(ContactId::user_id("user-1"), ContactFields::new())
        .await
        .unwrap_err();
    assert_api_error(&err, "Failed to upsert contact: Something went wrong", 400);
}

#[tokio::test]
async fn find_contact() {
    let (mock, client) = setup().await;
    let result = client
        .find_contact(ContactId::email("user@test.com"))
        .await
        .unwrap()
        .expect("contacts found");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id.as_deref(), Some("mock-contact-id"));
    let request = &mock.calls(Route::FindContact).await[0];
    assert_eq!(request.url.query(), Some("email=user%40test.com"));
}

#[tokio::test]
async fn find_contact_keeps_custom_properties() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::FindContact,
        200,
        json!([{
            "id": "c1",
            "email": "user@test.com",
            "firstName": "Ada",
            "subscribed": true,
            "mailingLists": { "list-1": true },
            "optInStatus": null,
            "plan": "pro",
            "seats": 3,
        }]),
    );
    let contact = client
        .find_contact(ContactId::email("user@test.com"))
        .await
        .unwrap()
        .unwrap()
        .remove(0);
    assert_eq!(contact.first_name.as_deref(), Some("Ada"));
    assert_eq!(contact.mailing_lists["list-1"], true);
    assert_eq!(contact.opt_in_status, None);
    assert_eq!(contact.properties.len(), 2, "{:?}", contact.properties);
    assert_eq!(contact.properties["plan"], "pro");
    assert_eq!(contact.properties["seats"], 3);
}

#[tokio::test]
async fn single_identifier_endpoints_reject_both() {
    let (mock, client) = setup().await;
    let both = || ContactId::both("user@test.com", "user-1");
    for err in [
        client.find_contact(both()).await.unwrap_err(),
        client.get_contact_suppression(both()).await.unwrap_err(),
        client.remove_contact_suppression(both()).await.unwrap_err(),
    ] {
        assert!(matches!(err, LoopsError::InvalidInput(_)), "{err:?}");
        assert!(err.to_string().contains("not both"), "{err}");
    }
    assert!(mock.server().received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn custom_properties_cannot_override_request_fields() {
    let (mock, client) = setup().await;
    let err = client
        .create_contact(
            "user@test.com",
            None,
            ContactFields::new().property("email", "x@test.com"),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, LoopsError::InvalidInput(_)), "{err:?}");
    let err = client
        .upsert_contact(
            ContactId::user_id("user-1"),
            ContactFields::new().property("firstName", "Sneaky"),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("`firstName`"), "{err}");
    let err = client
        .send_event(
            "signup",
            ContactId::user_id("user-1"),
            EventOptions::new().contact_property("eventName", "other"),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, LoopsError::InvalidInput(_)), "{err:?}");
    assert!(mock.server().received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn find_contact_failure_is_none() {
    let (mock, client) = setup().await;
    mock.respond_json(Route::FindContact, 400, failure());
    let result = client
        .find_contact(ContactId::user_id("user-1"))
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn find_contact_server_error_is_an_error() {
    let (mock, client) = setup().await;
    mock.respond_json(Route::FindContact, 500, failure());
    let err = client
        .find_contact(ContactId::user_id("user-1"))
        .await
        .unwrap_err();
    assert_api_error(&err, "Something went wrong", 500);
}

#[tokio::test]
async fn delete_contact() {
    let (mock, client) = setup().await;
    assert!(
        client
            .delete_contact(ContactId::email("user@test.com"))
            .await
            .unwrap()
    );
    assert_eq!(
        body(&mock, Route::DeleteContact).await,
        json!({ "email": "user@test.com" })
    );
}

#[tokio::test]
async fn delete_contact_by_user_id() {
    let (mock, client) = setup().await;
    assert!(
        client
            .delete_contact(ContactId::user_id("user-123"))
            .await
            .unwrap()
    );
    assert_eq!(
        body(&mock, Route::DeleteContact).await,
        json!({ "userId": "user-123" })
    );
}

#[tokio::test]
async fn delete_contact_rejects_both_identifiers() {
    let (mock, client) = setup().await;
    let err = client
        .delete_contact(ContactId::both("user@test.com", "user-123"))
        .await
        .unwrap_err();
    assert!(matches!(err, LoopsError::InvalidInput(_)), "{err:?}");
    assert!(err.to_string().contains("not both"));
    assert!(!mock.called(Route::DeleteContact).await);
}

#[tokio::test]
async fn delete_contact_not_found_is_false() {
    let (mock, client) = setup().await;
    mock.respond_json(Route::DeleteContact, 404, failure());
    assert!(
        !client
            .delete_contact(ContactId::email("gone@test.com"))
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn send_event() {
    let (mock, client) = setup().await;
    let result = client
        .send_event(
            "signup",
            ContactId::email("user@test.com"),
            EventOptions::new()
                .event_property("plan", "premium")
                .mailing_list("list-1", false)
                .contact_property("firstName", "John"),
        )
        .await
        .unwrap();
    assert!(result.success);
    let request = &mock.calls(Route::SendEvent).await[0];
    assert!(uuid_like(
        request.headers["idempotency-key"].to_str().unwrap()
    ));
    assert_eq!(
        request.body_json::<Value>().unwrap(),
        json!({
            "eventName": "signup",
            "email": "user@test.com",
            "eventProperties": { "plan": "premium" },
            "mailingLists": { "list-1": false },
            "firstName": "John",
        })
    );
}

#[tokio::test]
async fn send_event_by_user_id() {
    let (mock, client) = setup().await;
    client
        .send_event("signup", ContactId::user_id("usr_1"), EventOptions::new())
        .await
        .unwrap();
    assert_eq!(
        body(&mock, Route::SendEvent).await,
        json!({ "eventName": "signup", "userId": "usr_1" })
    );
}

#[tokio::test]
async fn send_event_idempotency_conflict() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::SendEvent,
        409,
        json!({ "success": false, "message": "Idempotency key has been used." }),
    );
    let err = client
        .send_event(
            "signup",
            ContactId::email("user@test.com"),
            EventOptions::new().idempotency_key("dup"),
        )
        .await
        .unwrap_err();
    assert_api_error(
        &err,
        "Idempotency key conflict: Idempotency key has been used.",
        409,
    );
}

#[tokio::test]
async fn list_contact_properties() {
    let (mock, client) = setup().await;
    assert_eq!(
        client.list_contact_properties().await.unwrap(),
        [] as [loops_so::generated::ContactProperty; 0]
    );
    assert!(mock.called(Route::ListContactProperties).await);
}

#[tokio::test]
async fn list_mailing_lists() {
    let (mock, client) = setup().await;
    assert_eq!(
        client.list_mailing_lists().await.unwrap(),
        [] as [loops_so::generated::MailingList; 0]
    );
    assert!(mock.called(Route::ListMailingLists).await);
}

#[tokio::test]
async fn create_contact_property() {
    let (mock, client) = setup().await;
    let result = client
        .create_contact_property("custom_prop", ContactPropertyCreateRequestType::String)
        .await
        .unwrap();
    assert_eq!(result["success"], true);
    assert_eq!(result["key"], "custom_prop");
    assert_eq!(result["type"], "string");
    assert_eq!(
        body(&mock, Route::CreateContactProperty).await,
        json!({ "name": "custom_prop", "type": "string" })
    );
}

#[tokio::test]
async fn list_transactional_emails() {
    let (mock, client) = setup().await;
    let result = client
        .list_transactional_emails(Page::first())
        .await
        .unwrap();
    assert_eq!(result.pagination.total_results, 0);
    assert_eq!(result.data, [] as [loops_so::TransactionalEmail; 0]);
    assert!(mock.called(Route::ListTransactional).await);
}

#[tokio::test]
async fn list_transactional_emails_paginates() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::ListTransactional,
        200,
        json!({
            "pagination": {
                "totalResults": 1, "returnedResults": 1, "perPage": 10, "totalPages": 1,
                "nextCursor": "c2", "nextPage": null,
            },
            "data": [{
                "id": "tpl_1", "name": "Welcome", "lastUpdated": "2024-01-01T00:00:00.000Z",
                "dataVariables": ["firstName"],
            }],
        }),
    );
    let result = client
        .list_transactional_emails(Page::first().per_page(10).cursor("c1"))
        .await
        .unwrap();
    assert_eq!(result.pagination.next_cursor.as_deref(), Some("c2"));
    assert_eq!(result.data[0].data_variables, ["firstName"]);
    let request = &mock.calls(Route::ListTransactional).await[0];
    assert_eq!(request.url.query(), Some("perPage=10&cursor=c1"));
}

#[tokio::test]
async fn list_sending_ips() {
    let (mock, client) = setup().await;
    assert_eq!(
        client.list_sending_ips().await.unwrap(),
        [] as [std::string::String; 0]
    );
    assert!(mock.called(Route::ListSendingIps).await);
}

// ---------------------------------------------------------------------------
// Named routes
// ---------------------------------------------------------------------------

const EXPECTED_ROUTE_NAMES: &[&str] = &[
    "health",
    "transactional",
    "list_transactional",
    "create_contact",
    "upsert_contact",
    "find_contact",
    "delete_contact",
    "list_contact_properties",
    "create_contact_property",
    "send_event",
    "list_mailing_lists",
    "list_sending_ips",
    // 1.8.0
    "list_campaigns",
    "get_campaign",
    "create_campaign",
    "update_campaign",
    "list_components",
    "get_component",
    "list_themes",
    "get_theme",
    "get_email_message",
    "update_email_message",
    "get_contact_suppression",
    "remove_contact_suppression",
    // 1.14.x
    "list_transactional_templates",
    "get_transactional_template",
    "create_transactional_template",
    "update_transactional_template",
    "draft_transactional_template",
    "publish_transactional_template",
    "create_upload",
    "complete_upload",
    "list_workflows",
    "get_workflow",
    "get_workflow_node",
    "list_audience_segments",
    "get_audience_segment",
    "list_campaign_groups",
    "get_campaign_group",
    "create_campaign_group",
    "update_campaign_group",
    "list_transactional_groups",
    "get_transactional_group",
    "create_transactional_group",
    "update_transactional_group",
    "preview_email_message",
    // 1.21.x
    "create_audience_segment",
    "create_component",
    "update_component",
    "create_theme",
    "update_theme",
    "get_email_message_guardian",
    "list_event_patterns",
    "get_event_pattern",
    "get_event_pattern_by_name",
    "create_workflow",
    "update_workflow",
    "change_workflow_mailing_list",
    "delete_workflow",
    "create_workflow_node",
    "update_workflow_node",
    "add_workflow_branch",
    "reroute_node_connection",
    "delete_workflow_node",
    "delete_workflow_node_recursively",
    // 1.22.x
    "get_campaign_metrics",
    "get_transactional_template_metrics",
    "get_workflow_node_metrics",
];

#[test]
fn named_routes_match_the_expected_list() {
    for name in EXPECTED_ROUTE_NAMES {
        assert!(Route::from_name(name).is_some(), "route {name:?} not found");
    }
    let names: Vec<&str> = Route::ALL.iter().map(|route| route.name()).collect();
    assert_eq!(names.len(), EXPECTED_ROUTE_NAMES.len(), "{names:?}");
}

// ---------------------------------------------------------------------------
// Error simulation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn override_429_rate_limit() {
    let (mock, client) = setup().await;
    mock.respond(Route::Transactional, rate_limited("10"));
    let err = client
        .send_transactional_email("abc", "user@test.com", TransactionalOptions::new())
        .await
        .unwrap_err();
    assert_rate_limit(&err, 10);
}

#[tokio::test]
async fn rate_limit_without_headers_reports_zero() {
    let (mock, client) = setup().await;
    mock.respond_json(Route::ListMailingLists, 429, json!({ "success": false }));
    match client.list_mailing_lists().await.unwrap_err() {
        LoopsError::RateLimit { limit, remaining } => assert_eq!((limit, remaining), (0, 0)),
        other => panic!("expected a rate limit error, got {other:?}"),
    }
}

#[tokio::test]
async fn override_400_bad_request() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::SendEvent,
        400,
        json!({ "success": false, "message": "Invalid event" }),
    );
    let err = client
        .send_event(
            "bad",
            ContactId::email("user@test.com"),
            EventOptions::new(),
        )
        .await
        .unwrap_err();
    assert_api_error(&err, "Failed to send event: Invalid event", 400);
}

#[tokio::test]
async fn override_404_transactional_not_found() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::Transactional,
        404,
        json!({
            "success": false,
            "message": "Transactional email not found",
            "error": { "statusCode": 404, "message": "Not found" },
        }),
    );
    let err = client
        .send_transactional_email("missing", "user@test.com", TransactionalOptions::new())
        .await
        .unwrap_err();
    assert_api_error(&err, "not found", 404);
    assert_eq!(err.response_body().unwrap()["error"]["statusCode"], 404);
}

#[tokio::test]
async fn override_400_transactional_not_published() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::Transactional,
        400,
        json!({ "success": false, "message": "Transactional email is not published" }),
    );
    let err = client
        .send_transactional_email("draft", "user@test.com", TransactionalOptions::new())
        .await
        .unwrap_err();
    assert_api_error(
        &err,
        "Failed to send transactional email: Transactional email is not published",
        400,
    );
}

#[tokio::test]
async fn latest_override_wins_however_many_there_are() {
    let (mock, client) = setup().await;
    for round in 0..60 {
        mock.respond_json(
            Route::ListSendingIps,
            200,
            json!([format!("10.0.0.{round}")]),
        );
    }
    assert_eq!(client.list_sending_ips().await.unwrap(), ["10.0.0.59"]);
    mock.reset_route(Route::ListSendingIps);
    assert_eq!(
        client.list_sending_ips().await.unwrap(),
        [] as [std::string::String; 0]
    );
}

#[tokio::test]
async fn latest_override_wins() {
    let (mock, client) = setup().await;
    mock.respond_json(Route::ListSendingIps, 400, failure());
    mock.respond_json(Route::ListSendingIps, 200, json!(["1.2.3.4"]));
    assert_eq!(client.list_sending_ips().await.unwrap(), ["1.2.3.4"]);
}

#[tokio::test]
async fn non_json_failure_body_still_maps_to_api_error() {
    let (mock, client) = setup().await;
    mock.respond(
        Route::ListMailingLists,
        ResponseTemplate::new(502).set_body_string("<html>Bad gateway</html>"),
    );
    let err = client.list_mailing_lists().await.unwrap_err();
    assert_api_error(&err, "Failed to list mailing lists", 502);
    assert!(err.response_body().is_none());
}

#[tokio::test]
async fn responses_tolerate_fields_the_spec_does_not_list_yet() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::CreateWorkflow,
        200,
        json!({
            "id": "mock-workflow-id",
            "url": "https://app.loops.so/workflows/mock-workflow-id",
            "workflowRevisionId": "rev",
            "status": "Draft",
            "name": "Mock Workflow",
            "mailingListId": null,
            "rootNodeId": "n1",
            "nodes": { "n1": { "typeName": "ExitAction", "nextNodeIds": [], "addedLater": true } },
            "addedLater": { "nested": 1 },
        }),
    );
    let workflow = client.create_workflow("x", None, None).await.unwrap();
    assert_eq!(workflow.id, "mock-workflow-id");
}

#[tokio::test]
async fn malformed_success_body_is_a_decode_error() {
    let (mock, client) = setup().await;
    mock.respond_json(Route::GetCampaign, 200, json!({ "id": 42 }));
    let err = client.get_campaign("c1").await.unwrap_err();
    assert!(
        matches!(
            err,
            LoopsError::Decode {
                action: "get campaign",
                status: 200,
                ..
            }
        ),
        "{err:?}"
    );
}

// ---------------------------------------------------------------------------
// Unmatched requests, singleton and mock lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unmatched_request_is_rejected() {
    let mock = LoopsMock::start().await;
    let response = reqwest::get(format!("{}/v1/unknown-endpoint", mock.base_url()))
        .await
        .unwrap();
    assert_eq!(response.status(), 501);
    let sent: Value = response.json().await.unwrap();
    assert!(
        sent["message"]
            .as_str()
            .unwrap()
            .contains("no mock route for GET /api/v1/unknown-endpoint")
    );
}

#[tokio::test]
async fn reset_client_clears_singleton() {
    let _mock = LoopsMock::start().await;
    let first = get_client().unwrap();
    assert!(Arc::ptr_eq(&first, &get_client().unwrap()));
    reset_client();
    let second = get_client().unwrap();
    assert!(!Arc::ptr_eq(&first, &second));
}

#[tokio::test]
async fn mock_configures_api_key() {
    let mock = LoopsMock::builder()
        .api_key("custom-test-key")
        .start()
        .await;
    get_client().unwrap().health().await.unwrap();
    let request = &mock.calls(Route::Health).await[0];
    assert_eq!(request.headers["authorization"], "Bearer custom-test-key");
}

#[tokio::test]
async fn mock_resets_singleton() {
    {
        let _mock = LoopsMock::start().await;
        get_client().unwrap();
        assert!(loops_so::testing::client_is_cached());
    }
    let _next = LoopsMock::start().await;
    assert!(!loops_so::testing::client_is_cached());
}

#[tokio::test]
async fn mock_client_is_independent_of_singleton() {
    let mock = LoopsMock::start().await;
    let client = mock.client();
    assert!(client.health().await.unwrap());
    assert!(!loops_so::testing::client_is_cached());
}

#[tokio::test]
async fn legacy_v1_base_url_still_matches_routes() {
    let mock = LoopsMock::start().await;
    configure(ConfigureOptions::new().base_url(format!("{}/v1/", mock.base_url())));
    reset_client();
    assert!(get_client().unwrap().health().await.unwrap());
    assert!(mock.called(Route::Health).await);
}

#[tokio::test]
async fn low_level_client_shares_base_url_and_key() {
    let (mock, client) = setup().await;
    let low_level = client.low_level().unwrap();
    let response = low_level
        .test_api_key(loops_so::generated::TestApiKeyRequest {})
        .await
        .unwrap();
    assert!(matches!(
        response,
        loops_so::generated::TestApiKeyResponse::Ok(_)
    ));
    let request = &mock.calls(Route::Health).await[0];
    assert_eq!(request.headers["authorization"], "Bearer test-key");
}

// ---------------------------------------------------------------------------
// Campaigns
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_campaigns() {
    let (mock, client) = setup().await;
    assert_eq!(
        client.list_campaigns(Page::first()).await.unwrap().data,
        [] as [loops_so::generated::CampaignResponse; 0]
    );
    assert!(mock.called(Route::ListCampaigns).await);
}

#[tokio::test]
async fn get_campaign() {
    let (mock, client) = setup().await;
    let result = client.get_campaign("mock-campaign-id").await.unwrap();
    assert_eq!(result.id, "mock-campaign-id");
    assert_eq!(
        result.url,
        "https://app.loops.so/campaigns/mock-campaign-id"
    );
    let request = &mock.calls(Route::GetCampaign).await[0];
    assert_eq!(request.url.path(), "/api/v1/campaigns/mock-campaign-id");
}

#[tokio::test]
async fn path_parameters_are_percent_encoded() {
    let (mock, client) = setup().await;
    client.get_campaign("a/b c").await.unwrap();
    let request = &mock.calls(Route::GetCampaign).await[0];
    assert_eq!(request.url.path(), "/api/v1/campaigns/a%2Fb%20c");
}

#[tokio::test]
async fn empty_path_parameter_is_rejected_before_sending() {
    let (mock, client) = setup().await;
    let err = client.get_campaign("").await.unwrap_err();
    assert!(matches!(err, LoopsError::InvalidInput(_)), "{err:?}");
    let err = client.get_workflow_node("wf", "").await.unwrap_err();
    assert!(matches!(err, LoopsError::InvalidInput(_)), "{err:?}");
    assert!(mock.server().received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn dot_path_parameters_are_rejected_before_sending() {
    let (mock, client) = setup().await;
    for id in [".", ".."] {
        let err = client
            .update_campaign_group(id, Some("Renamed"), None)
            .await
            .unwrap_err();
        assert!(matches!(err, LoopsError::InvalidInput(_)), "{id}: {err:?}");
    }
    assert!(mock.server().received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn create_campaign() {
    let (mock, client) = setup().await;
    let result = client.create_campaign("My Campaign").await.unwrap();
    assert_eq!(result.id, "mock-campaign-id");
    assert_eq!(
        result.email_message_id.as_deref(),
        Some("mock-email-message-id")
    );
    assert_eq!(
        body(&mock, Route::CreateCampaign).await,
        json!({ "name": "My Campaign" })
    );
}

#[tokio::test]
async fn update_campaign() {
    let (mock, client) = setup().await;
    let result = client
        .update_campaign("mock-campaign-id", "Updated Campaign")
        .await
        .unwrap();
    assert_eq!(result.name, "Updated Campaign");
    assert_eq!(
        body(&mock, Route::UpdateCampaign).await,
        json!({ "name": "Updated Campaign" })
    );
}

#[tokio::test]
async fn campaigns_rate_limit() {
    let (mock, client) = setup().await;
    mock.respond(Route::ListCampaigns, rate_limited("5"));
    let err = client.list_campaigns(Page::first()).await.unwrap_err();
    assert_rate_limit(&err, 5);
}

#[tokio::test]
async fn get_campaign_metrics() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::GetCampaignMetrics,
        200,
        json!({
            "sends": 4210, "opens": 1922, "clicks": 301, "unsubscribes": 12,
            "spamReports": 1, "hardBounces": 9, "softBounces": 13,
        }),
    );
    let result = client
        .get_campaign_metrics("mock-campaign-id")
        .await
        .unwrap();
    assert_eq!(result.sends, 4210);
    assert_eq!(result.opens, 1922);
    assert_eq!(result.clicks, 301);
    assert_eq!(result.unsubscribes, 12);
    assert_eq!(result.spam_reports, 1);
    assert_eq!(result.hard_bounces, 9);
    assert_eq!(result.soft_bounces, 13);
    let request = &mock.calls(Route::GetCampaignMetrics).await[0];
    assert_eq!(
        request.url.path(),
        "/api/v1/campaigns/mock-campaign-id/metrics"
    );
    assert!(!mock.called(Route::GetCampaign).await);
}

#[tokio::test]
async fn get_campaign_metrics_404() {
    let (mock, client) = setup().await;
    mock.respond_json(Route::GetCampaignMetrics, 404, failure());
    let err = client.get_campaign_metrics("missing-id").await.unwrap_err();
    assert_api_error(&err, "Something went wrong", 404);
}

// ---------------------------------------------------------------------------
// Components and themes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_components() {
    let (mock, client) = setup().await;
    assert_eq!(
        client.list_components(Page::first()).await.unwrap().data,
        [] as [loops_so::generated::Component; 0]
    );
    assert!(mock.called(Route::ListComponents).await);
}

#[tokio::test]
async fn get_component() {
    let (_mock, client) = setup().await;
    let result = client.get_component("mock-component-id").await.unwrap();
    assert_eq!(result.id, "mock-component-id");
    assert_eq!(result.name, "Mock Component");
}

#[tokio::test]
async fn create_component() {
    let (mock, client) = setup().await;
    let result = client
        .create_component("Hero", "<Text>Hi</Text>")
        .await
        .unwrap();
    assert_eq!(result.id, "mock-component-id");
    assert_eq!(
        body(&mock, Route::CreateComponent).await,
        json!({ "name": "Hero", "lmx": "<Text>Hi</Text>" })
    );
}

#[tokio::test]
async fn update_component() {
    let (mock, client) = setup().await;
    let result = client
        .update_component("mock-component-id", None, Some("<Text>Bye</Text>"))
        .await
        .unwrap();
    assert_eq!(result.affected_email_count, 0.0);
    assert_eq!(
        body(&mock, Route::UpdateComponent).await,
        json!({ "lmx": "<Text>Bye</Text>" })
    );
}

#[tokio::test]
async fn list_themes() {
    let (mock, client) = setup().await;
    assert_eq!(
        client.list_themes(Page::first()).await.unwrap().data,
        [] as [loops_so::generated::Theme; 0]
    );
    assert!(mock.called(Route::ListThemes).await);
}

#[tokio::test]
async fn get_theme() {
    let (_mock, client) = setup().await;
    let result = client.get_theme("mock-theme-id").await.unwrap();
    assert_eq!(result.id, "mock-theme-id");
    assert_eq!(result.name, "Mock Theme");
}

#[tokio::test]
async fn create_theme() {
    let (mock, client) = setup().await;
    let styles = ThemeStyles {
        body_color: Some("#fff".to_owned()),
        ..Default::default()
    };
    let result = client.create_theme("Brand", Some(styles)).await.unwrap();
    assert_eq!(result.id, "mock-theme-id");
    assert_eq!(
        body(&mock, Route::CreateTheme).await,
        json!({ "name": "Brand", "styles": { "bodyColor": "#fff" } })
    );
}

#[tokio::test]
async fn update_theme() {
    let (mock, client) = setup().await;
    let result = client
        .update_theme("mock-theme-id", Some("Brand 2"), None)
        .await
        .unwrap();
    assert_eq!(result.affected_email_count, 0.0);
    assert_eq!(
        body(&mock, Route::UpdateTheme).await,
        json!({ "name": "Brand 2" })
    );
}

// ---------------------------------------------------------------------------
// Email messages
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_email_message() {
    let (_mock, client) = setup().await;
    let result = client
        .get_email_message("mock-email-message-id")
        .await
        .unwrap();
    assert_eq!(result.id, "mock-email-message-id");
    assert_eq!(result.subject, "Mock Subject");
}

#[tokio::test]
async fn update_email_message() {
    let (mock, client) = setup().await;
    let result = client
        .update_email_message(
            "mock-email-message-id",
            EmailMessageUpdate::new()
                .subject("New Subject")
                .from_name("Alice"),
        )
        .await
        .unwrap();
    assert_eq!(result.id, "mock-email-message-id");
    assert_eq!(
        body(&mock, Route::UpdateEmailMessage).await,
        json!({ "subject": "New Subject", "fromName": "Alice" })
    );
}

#[tokio::test]
async fn update_email_message_sender_format_and_fallbacks() {
    let (mock, client) = setup().await;
    client
        .update_email_message(
            "mock-email-message-id",
            EmailMessageUpdate::new()
                .cc_email("cc@test.com")
                .bcc_email("")
                .language_code("fr")
                .email_format(EmailFormat::Plain)
                .contact_property_fallback("firstName", Some("there"))
                .contact_property_fallback("company", None)
                .event_property_fallback("plan", Some("free"))
                .data_variable_fallback("code", None),
        )
        .await
        .unwrap();
    // None is sent as null: it deletes that key's fallback server-side.
    assert_eq!(
        body(&mock, Route::UpdateEmailMessage).await,
        json!({
            "ccEmail": "cc@test.com",
            "bccEmail": "",
            "languageCode": "fr",
            "emailFormat": "plain",
            "contactPropertiesFallbacks": { "company": null, "firstName": "there" },
            "eventPropertiesFallbacks": { "plan": "free" },
            "dataVariablesFallbacks": { "code": null },
        })
    );
}

#[tokio::test]
async fn get_email_message_guardian() {
    let (mock, client) = setup().await;
    let result = client
        .get_email_message_guardian("mock-email-message-id")
        .await
        .unwrap();
    assert_eq!(result.errors, [] as [loops_so::generated::GuardianRule; 0]);
    assert_eq!(
        result.warnings,
        [] as [loops_so::generated::GuardianRule; 0]
    );
    assert!(mock.called(Route::GetEmailMessageGuardian).await);
}

#[tokio::test]
async fn preview_email_message() {
    let (mock, client) = setup().await;
    let result = client
        .preview_email_message(
            "mock-email-message-id",
            &["user@test.com"],
            PreviewOptions::new()
                .data_variable("name", "Jan")
                .data_variable("count", 2),
        )
        .await
        .unwrap();
    assert_eq!(result.id, "mock-preview-id");
    assert_eq!(
        body(&mock, Route::PreviewEmailMessage).await,
        json!({ "emails": ["user@test.com"], "dataVariables": { "name": "Jan", "count": 2 } })
    );
}

#[tokio::test]
async fn preview_email_message_needs_a_recipient() {
    let (mock, client) = setup().await;
    let err = client
        .preview_email_message("mock-email-message-id", &[], PreviewOptions::new())
        .await
        .unwrap_err();
    assert!(matches!(err, LoopsError::InvalidInput(_)), "{err:?}");
    assert!(!mock.called(Route::PreviewEmailMessage).await);
}

// ---------------------------------------------------------------------------
// Contact suppression
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_contact_suppression() {
    let (mock, client) = setup().await;
    let result = client
        .get_contact_suppression(ContactId::email("user@test.com"))
        .await
        .unwrap();
    assert!(!result.is_suppressed);
    assert_eq!(result.removal_quota.remaining, 3.0);
    let request = &mock.calls(Route::GetContactSuppression).await[0];
    assert_eq!(request.url.query(), Some("email=user%40test.com"));
}

#[tokio::test]
async fn remove_contact_suppression() {
    let (mock, client) = setup().await;
    let result = client
        .remove_contact_suppression(ContactId::user_id("user-1"))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.removal_quota.remaining, 2.0);
    let request = &mock.calls(Route::RemoveContactSuppression).await[0];
    assert_eq!(request.url.query(), Some("userId=user-1"));
}

// ---------------------------------------------------------------------------
// Transactional templates and uploads
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_transactional_templates() {
    let (mock, client) = setup().await;
    assert_eq!(
        client
            .list_transactional_templates(Page::first())
            .await
            .unwrap()
            .data,
        [] as [loops_so::generated::TransactionalResource; 0]
    );
    assert!(mock.called(Route::ListTransactionalTemplates).await);
}

#[tokio::test]
async fn get_transactional_template() {
    let (_mock, client) = setup().await;
    let result = client
        .get_transactional_template("mock-transactional-id")
        .await
        .unwrap();
    assert_eq!(result.id, "mock-transactional-id");
    assert_eq!(
        result.url,
        "https://app.loops.so/transactional/mock-transactional-id"
    );
}

#[tokio::test]
async fn get_transactional_variables() {
    let (mock, client) = setup().await;
    assert_eq!(
        client
            .get_transactional_variables("mock-transactional-id")
            .await
            .unwrap(),
        [] as [std::string::String; 0]
    );
    assert!(mock.called(Route::GetTransactionalTemplate).await);
}

#[tokio::test]
async fn get_transactional_variables_returns_template_names() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::GetTransactionalTemplate,
        200,
        json!({
            "id": "mock-transactional-id",
            "url": "https://app.loops.so/transactional/mock-transactional-id",
            "name": "Invitation",
            "draftEmailMessageId": null,
            "publishedEmailMessageId": "mock-email-message-id",
            "transactionalGroupId": null,
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z",
            "dataVariables": ["firstName", "inviteLink"],
        }),
    );
    assert_eq!(
        client
            .get_transactional_variables("mock-transactional-id")
            .await
            .unwrap(),
        ["firstName", "inviteLink"]
    );
}

#[tokio::test]
async fn get_transactional_variables_not_found() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::GetTransactionalTemplate,
        404,
        json!({ "message": "Not found" }),
    );
    let err = client
        .get_transactional_variables("missing-id")
        .await
        .unwrap_err();
    assert_api_error(&err, "Not found", 404);
}

#[tokio::test]
async fn get_transactional_template_metrics() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::GetTransactionalTemplateMetrics,
        200,
        json!({ "sends": 4210, "deliveries": 4175, "spamReports": 1, "hardBounces": 9, "softBounces": 13 }),
    );
    let result = client
        .get_transactional_template_metrics("mock-transactional-id")
        .await
        .unwrap();
    assert_eq!(result.sends, 4210);
    assert_eq!(result.deliveries, 4175);
    assert_eq!(result.spam_reports, 1);
    assert_eq!(result.hard_bounces, 9);
    assert_eq!(result.soft_bounces, 13);
    assert!(!mock.called(Route::GetTransactionalTemplate).await);
}

#[tokio::test]
async fn create_transactional_template() {
    let (mock, client) = setup().await;
    let result = client
        .create_transactional_template("My Template", Some("grp-1"))
        .await
        .unwrap();
    assert_eq!(result.id, "mock-transactional-id");
    assert_eq!(
        body(&mock, Route::CreateTransactionalTemplate).await,
        json!({ "name": "My Template", "transactionalGroupId": "grp-1" })
    );
}

#[tokio::test]
async fn update_transactional_template() {
    let (mock, client) = setup().await;
    let result = client
        .update_transactional_template("mock-transactional-id", Some("Renamed"), None)
        .await
        .unwrap();
    assert_eq!(result.id, "mock-transactional-id");
    assert_eq!(
        body(&mock, Route::UpdateTransactionalTemplate).await,
        json!({ "name": "Renamed" })
    );
}

#[tokio::test]
async fn draft_transactional_template() {
    let (mock, client) = setup().await;
    let result = client
        .draft_transactional_template("mock-transactional-id")
        .await
        .unwrap();
    assert_eq!(result.id, "mock-transactional-id");
    assert!(mock.called(Route::DraftTransactionalTemplate).await);
    assert!(!mock.called(Route::UpdateTransactionalTemplate).await);
}

#[tokio::test]
async fn publish_transactional_template() {
    let (mock, client) = setup().await;
    let result = client
        .publish_transactional_template("mock-transactional-id")
        .await
        .unwrap();
    assert_eq!(result.id, "mock-transactional-id");
    assert!(mock.called(Route::PublishTransactionalTemplate).await);
}

#[tokio::test]
async fn create_upload() {
    let (mock, client) = setup().await;
    let result = client.create_upload("image/png", 1234).await.unwrap();
    assert_eq!(result.email_asset_id, "mock-asset-id");
    assert_eq!(
        body(&mock, Route::CreateUpload).await,
        json!({ "contentType": "image/png", "contentLength": 1234 })
    );
}

#[tokio::test]
async fn complete_upload() {
    let (mock, client) = setup().await;
    let result = client.complete_upload("mock-asset-id").await.unwrap();
    assert_eq!(result.final_url, "https://example.com/final.png");
    assert!(mock.called(Route::CompleteUpload).await);
}

#[tokio::test]
async fn complete_upload_limit_exceeded() {
    // Loops reports an exceeded upload limit as HTTP 429, which surfaces
    // uniformly as a rate limit.
    let (mock, client) = setup().await;
    mock.respond(
        Route::CompleteUpload,
        ResponseTemplate::new(429)
            .set_body_json(json!({ "message": "Upload limit exceeded" }))
            .insert_header("x-ratelimit-limit", "100")
            .insert_header("x-ratelimit-remaining", "0"),
    );
    let err = client.complete_upload("mock-asset-id").await.unwrap_err();
    assert_rate_limit(&err, 100);
}

#[tokio::test]
async fn complete_upload_error() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::CompleteUpload,
        400,
        json!({ "message": "Bad upload" }),
    );
    let err = client.complete_upload("mock-asset-id").await.unwrap_err();
    assert_api_error(&err, "Bad upload", 400);
}

// ---------------------------------------------------------------------------
// Workflows and workflow nodes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_workflows() {
    let (mock, client) = setup().await;
    assert_eq!(
        client.list_workflows(Page::first()).await.unwrap().data,
        [] as [loops_so::generated::WorkflowSummary; 0]
    );
    assert!(mock.called(Route::ListWorkflows).await);
}

#[tokio::test]
async fn list_workflows_rate_limit() {
    let (mock, client) = setup().await;
    mock.respond(Route::ListWorkflows, rate_limited("7"));
    let err = client.list_workflows(Page::first()).await.unwrap_err();
    assert_rate_limit(&err, 7);
}

#[tokio::test]
async fn get_workflow() {
    let (_mock, client) = setup().await;
    let result = client.get_workflow("mock-workflow-id").await.unwrap();
    assert_eq!(result.id, "mock-workflow-id");
    assert_eq!(
        result.url,
        "https://app.loops.so/workflows/mock-workflow-id"
    );
}

#[tokio::test]
async fn get_workflow_decodes_each_node_by_type_name() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::GetWorkflow,
        200,
        json!({
            "id": "wf",
            "url": "https://app.loops.so/workflows/wf",
            "workflowRevisionId": "rev",
            "status": "Draft",
            "name": "Onboarding",
            "description": null,
            "mailingListId": null,
            "rootNodeId": "n1",
            "nodes": {
                "n1": { "typeName": "SignupTrigger", "nextNodeIds": ["n2"] },
                "n2": { "typeName": "TimerAction", "nextNodeIds": ["n3"], "amount": 2, "unit": "d" },
                "n3": { "typeName": "BranchNode", "nextNodeIds": ["n4"] },
                "n4": { "typeName": "ExitAction", "nextNodeIds": [] },
            },
        }),
    );
    let workflow = client.get_workflow("wf").await.unwrap();
    let kinds: Vec<&str> = workflow
        .nodes
        .values()
        .map(|node| match node.as_ref() {
            SimplifiedWorkflowNode::SignupTrigger(_) => "SignupTrigger",
            SimplifiedWorkflowNode::TimerAction(_) => "TimerAction",
            SimplifiedWorkflowNode::Branch(_) => "BranchNode",
            SimplifiedWorkflowNode::ExitAction(_) => "ExitAction",
            other => panic!("unexpected variant {other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        ["SignupTrigger", "TimerAction", "BranchNode", "ExitAction"]
    );
}

#[tokio::test]
async fn get_workflow_node_with_unknown_type_name_is_a_decode_error() {
    let (mock, client) = setup().await;
    let mut workflow = loops_so::testing::wiremock::ResponseTemplate::new(200);
    workflow = workflow.set_body_json(json!({
        "id": "wf", "url": "u", "workflowRevisionId": null, "status": "Draft",
        "name": "x", "mailingListId": null, "rootNodeId": "n1",
        "nodes": { "n1": { "typeName": "Teleport", "nextNodeIds": [] } },
    }));
    mock.respond(Route::GetWorkflow, workflow);
    let err = client.get_workflow("wf").await.unwrap_err();
    assert!(matches!(err, LoopsError::Decode { .. }), "{err:?}");
}

#[tokio::test]
async fn get_workflow_not_found() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::GetWorkflow,
        404,
        json!({ "message": "Workflow not found" }),
    );
    let err = client.get_workflow("missing").await.unwrap_err();
    assert_api_error(&err, "Workflow not found", 404);
}

#[tokio::test]
async fn create_workflow() {
    let (mock, client) = setup().await;
    let result = client
        .create_workflow("Onboarding", Some("desc"), Some("list-1"))
        .await
        .unwrap();
    assert_eq!(result.id, "mock-workflow-id");
    assert_eq!(
        body(&mock, Route::CreateWorkflow).await,
        json!({ "name": "Onboarding", "description": "desc", "mailingListId": "list-1" })
    );
}

#[tokio::test]
async fn create_workflow_rate_limit() {
    let (mock, client) = setup().await;
    mock.respond(Route::CreateWorkflow, rate_limited("10"));
    let err = client.create_workflow("x", None, None).await.unwrap_err();
    assert_rate_limit(&err, 10);
}

#[tokio::test]
async fn update_workflow() {
    let (mock, client) = setup().await;
    let result = client
        .update_workflow("mock-workflow-id", Some("rev-1"), Some("Renamed"), None)
        .await
        .unwrap();
    assert_eq!(result.id, "mock-workflow-id");
    assert_eq!(
        body(&mock, Route::UpdateWorkflow).await,
        json!({ "expectedRevisionId": "rev-1", "name": "Renamed" })
    );
}

#[tokio::test]
async fn update_workflow_sends_null_revision() {
    let (mock, client) = setup().await;
    client
        .update_workflow("mock-workflow-id", None, None, Some("d"))
        .await
        .unwrap();
    assert_eq!(
        body(&mock, Route::UpdateWorkflow).await,
        json!({ "expectedRevisionId": null, "description": "d" })
    );
}

#[tokio::test]
async fn change_workflow_mailing_list() {
    let (mock, client) = setup().await;
    let result = client
        .change_workflow_mailing_list(
            "mock-workflow-id",
            Some("rev-1"),
            Some("list-1"),
            None,
            None,
        )
        .await
        .unwrap();
    match result {
        ChangeWorkflowMailingListResponse::UpdatedResponse(updated) => {
            assert_eq!(updated.mailing_list_id.as_deref(), Some("mock-list-id"));
        }
        other @ ChangeWorkflowMailingListResponse::Preview(_) => {
            panic!("expected an applied change, got {other:?}")
        }
    }
    assert_eq!(
        body(&mock, Route::ChangeWorkflowMailingList).await,
        json!({ "expectedRevisionId": "rev-1", "mailingListId": "list-1" })
    );
}

#[tokio::test]
async fn change_workflow_mailing_list_clears_with_null() {
    let (mock, client) = setup().await;
    client
        .change_workflow_mailing_list(
            "mock-workflow-id",
            None,
            None,
            Some(true),
            Some(loops_so::generated::WorkflowQueuedContactPolicy::Discard),
        )
        .await
        .unwrap();
    assert_eq!(
        body(&mock, Route::ChangeWorkflowMailingList).await,
        json!({
            "expectedRevisionId": null,
            "mailingListId": null,
            "dryRun": true,
            "queuedContactPolicy": "discard",
        })
    );
}

#[tokio::test]
async fn delete_workflow() {
    let (mock, client) = setup().await;
    assert!(
        client
            .delete_workflow("mock-workflow-id", Some("rev-1"), None)
            .await
            .unwrap()
    );
    assert_eq!(
        body(&mock, Route::DeleteWorkflow).await,
        json!({ "expectedRevisionId": "rev-1" })
    );
}

#[tokio::test]
async fn delete_workflow_confirmation_required() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::DeleteWorkflow,
        409,
        json!({ "message": "Workflow is sending. Retry with confirmDelete." }),
    );
    let err = client
        .delete_workflow("mock-workflow-id", Some("rev-1"), None)
        .await
        .unwrap_err();
    assert_api_error(&err, "Retry with confirmDelete", 409);
}

#[tokio::test]
async fn delete_workflow_confirmed() {
    let (mock, client) = setup().await;
    assert!(
        client
            .delete_workflow("mock-workflow-id", Some("rev-1"), Some(true))
            .await
            .unwrap()
    );
    assert_eq!(
        body(&mock, Route::DeleteWorkflow).await,
        json!({ "expectedRevisionId": "rev-1", "confirmDelete": true })
    );
}

#[tokio::test]
async fn delete_workflow_not_found() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::DeleteWorkflow,
        404,
        json!({ "message": "Workflow not found" }),
    );
    let err = client
        .delete_workflow("missing", None, None)
        .await
        .unwrap_err();
    assert_api_error(&err, "Workflow not found", 404);
}

#[tokio::test]
async fn get_workflow_node() {
    let (mock, client) = setup().await;
    let result = client
        .get_workflow_node("mock-workflow-id", "mock-node-id")
        .await
        .unwrap();
    assert_eq!(result["id"], "mock-node-id");
    assert_eq!(result["typeName"], "ExitAction");
    assert!(mock.called(Route::GetWorkflowNode).await);
}

#[tokio::test]
async fn get_workflow_node_metrics() {
    let (mock, client) = setup().await;
    let result = client
        .get_workflow_node_metrics("mock-workflow-id", "mock-node-id")
        .await
        .unwrap();
    assert_eq!(result.sends, 0);
    assert_eq!(result.opens, 0);
    let request = &mock.calls(Route::GetWorkflowNodeMetrics).await[0];
    assert_eq!(
        request.url.path(),
        "/api/v1/workflows/mock-workflow-id/nodes/mock-node-id/metrics"
    );
    assert!(!mock.called(Route::GetWorkflowNode).await);
}

#[tokio::test]
async fn get_workflow_node_metrics_not_an_email_node() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::GetWorkflowNodeMetrics,
        400,
        json!({ "message": "Node is not a SendEmailAction node" }),
    );
    let err = client
        .get_workflow_node_metrics("wf", "timer-node")
        .await
        .unwrap_err();
    assert_api_error(&err, "SendEmailAction", 400);
}

#[tokio::test]
async fn create_workflow_node_between() {
    let (mock, client) = setup().await;
    let result = client
        .create_workflow_node(
            "mock-workflow-id",
            CreateWorkflowNodeTypeName::TimerAction,
            Some("rev-1"),
            NodePlacement::between("n1", "n2"),
        )
        .await
        .unwrap();
    assert_eq!(result["workflow"]["id"], "mock-workflow-id");
    assert_eq!(
        body(&mock, Route::CreateWorkflowNode).await,
        json!({
            "expectedRevisionId": "rev-1",
            "nodeTypeName": "TimerAction",
            "insertMode": "between",
            "fromNodeId": "n1",
            "toNodeId": "n2",
        })
    );
}

#[tokio::test]
async fn create_workflow_node_before() {
    let (mock, client) = setup().await;
    client
        .create_workflow_node(
            "mock-workflow-id",
            CreateWorkflowNodeTypeName::TimerAction,
            Some("rev-1"),
            NodePlacement::before("n2"),
        )
        .await
        .unwrap();
    assert_eq!(
        body(&mock, Route::CreateWorkflowNode).await,
        json!({
            "expectedRevisionId": "rev-1",
            "insertMode": "before",
            "nodeTypeName": "TimerAction",
            "toNodeId": "n2",
        })
    );
}

#[tokio::test]
async fn create_workflow_node_after() {
    let (mock, client) = setup().await;
    client
        .create_workflow_node(
            "mock-workflow-id",
            CreateWorkflowNodeTypeName::SendEmailAction,
            None,
            NodePlacement::after("n1"),
        )
        .await
        .unwrap();
    assert_eq!(
        body(&mock, Route::CreateWorkflowNode).await,
        json!({
            "expectedRevisionId": null,
            "insertMode": "after",
            "nodeTypeName": "SendEmailAction",
            "fromNodeId": "n1",
        })
    );
}

#[tokio::test]
async fn update_workflow_node() {
    let (mock, client) = setup().await;
    let result = client
        .update_workflow_node(
            "mock-workflow-id",
            "mock-node-id",
            Some("rev-1"),
            json!({ "typeName": "SignupTrigger" }),
        )
        .await
        .unwrap();
    assert_eq!(result["id"], "mock-node-id");
    assert_eq!(result["workflow"]["id"], "mock-workflow-id");
    assert_eq!(
        body(&mock, Route::UpdateWorkflowNode).await,
        json!({ "expectedRevisionId": "rev-1", "payload": { "typeName": "SignupTrigger" } })
    );
}

#[tokio::test]
async fn reroute_workflow_node_connection() {
    let (mock, client) = setup().await;
    let result = client
        .reroute_workflow_node_connection("mock-workflow-id", "mock-node-id", Some("rev-1"), "n9")
        .await
        .unwrap();
    assert_eq!(result["id"], "mock-node-id");
    assert_eq!(result["workflow"]["id"], "mock-workflow-id");
    assert_eq!(
        body(&mock, Route::RerouteNodeConnection).await,
        json!({ "expectedRevisionId": "rev-1", "newTargetNodeId": "n9" })
    );
}

#[tokio::test]
async fn reroute_workflow_node_connection_not_found() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::RerouteNodeConnection,
        404,
        json!({ "message": "Not found" }),
    );
    let err = client
        .reroute_workflow_node_connection("wf", "n1", Some("rev"), "n9")
        .await
        .unwrap_err();
    assert_api_error(&err, "Not found", 404);
}

#[tokio::test]
async fn add_workflow_branch() {
    let (mock, client) = setup().await;
    let result = client
        .add_workflow_branch("mock-workflow-id", "mock-node-id", Some("rev-1"))
        .await
        .unwrap();
    assert_eq!(result.workflow.id, "mock-workflow-id");
    assert_eq!(
        body(&mock, Route::AddWorkflowBranch).await,
        json!({ "expectedRevisionId": "rev-1" })
    );
}

#[tokio::test]
async fn delete_workflow_node() {
    let (mock, client) = setup().await;
    let result = client
        .delete_workflow_node(
            "mock-workflow-id",
            "mock-node-id",
            Some("rev-1"),
            DeleteNodeOptions::new(),
        )
        .await
        .unwrap();
    assert_deleted(&result);
    assert!(mock.called(Route::DeleteWorkflowNode).await);
    assert_eq!(
        body(&mock, Route::DeleteWorkflowNode).await,
        json!({ "expectedRevisionId": "rev-1" })
    );
}

#[tokio::test]
async fn delete_workflow_node_recursively() {
    let (mock, client) = setup().await;
    let result = client
        .delete_workflow_node(
            "mock-workflow-id",
            "mock-node-id",
            Some("rev-1"),
            DeleteNodeOptions::new()
                .recursive(true)
                .dry_run(false)
                .queued_contact_policy(loops_so::generated::WorkflowQueuedContactPolicy::Fail),
        )
        .await
        .unwrap();
    assert_deleted(&result);
    assert!(mock.called(Route::DeleteWorkflowNodeRecursively).await);
    assert!(!mock.called(Route::DeleteWorkflowNode).await);
    assert_eq!(
        body(&mock, Route::DeleteWorkflowNodeRecursively).await,
        json!({ "expectedRevisionId": "rev-1", "dryRun": false, "queuedContactPolicy": "fail" })
    );
}

#[track_caller]
fn assert_deleted(result: &DeleteWorkflowNodeResponse) {
    match result {
        DeleteWorkflowNodeResponse::DeletedResponse(deleted) => {
            assert_eq!(deleted.node_ids, ["mock-node-id"]);
        }
        other @ DeleteWorkflowNodeResponse::QueuedContactDeletePreview(_) => {
            panic!("expected a deletion, got {other:?}")
        }
    }
}

// ---------------------------------------------------------------------------
// Event patterns, audience segments and groups
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_event_patterns() {
    let (mock, client) = setup().await;
    assert_eq!(
        client
            .list_event_patterns(Page::first())
            .await
            .unwrap()
            .data,
        [] as [loops_so::generated::EventPatternSummary; 0]
    );
    assert!(mock.called(Route::ListEventPatterns).await);
}

#[tokio::test]
async fn get_event_pattern() {
    let (mock, client) = setup().await;
    let result = client
        .get_event_pattern("mock-event-pattern-id")
        .await
        .unwrap();
    assert_eq!(result.id, "mock-event-pattern-id");
    assert_eq!(result.event_name, "signup");
    assert!(!mock.called(Route::GetEventPatternByName).await);
}

#[tokio::test]
async fn get_event_pattern_by_name() {
    let (mock, client) = setup().await;
    let result = client.get_event_pattern_by_name("signup").await.unwrap();
    assert_eq!(result.event_name, "signup");
    assert!(mock.called(Route::GetEventPatternByName).await);
    assert!(!mock.called(Route::GetEventPattern).await);
}

#[tokio::test]
async fn list_audience_segments() {
    let (mock, client) = setup().await;
    assert_eq!(
        client
            .list_audience_segments(Page::first())
            .await
            .unwrap()
            .data,
        [] as [loops_so::generated::AudienceSegment; 0]
    );
    assert!(mock.called(Route::ListAudienceSegments).await);
}

#[tokio::test]
async fn get_audience_segment() {
    let (_mock, client) = setup().await;
    let result = client
        .get_audience_segment("mock-segment-id")
        .await
        .unwrap();
    assert_eq!(result.id, "mock-segment-id");
}

#[tokio::test]
async fn create_audience_segment() {
    let (mock, client) = setup().await;
    let filter = json!({
        "match": "all",
        "conditions": [{ "type": "property", "key": "plan", "operator": "equals", "value": "pro" }],
    });
    let result = client
        .create_audience_segment("Pro users", filter.clone(), Some("desc"))
        .await
        .unwrap();
    assert_eq!(result.id, "mock-segment-id");
    assert_eq!(
        body(&mock, Route::CreateAudienceSegment).await,
        json!({ "name": "Pro users", "filter": filter, "description": "desc" })
    );
}

#[tokio::test]
async fn list_campaign_groups() {
    let (mock, client) = setup().await;
    assert_eq!(
        client
            .list_campaign_groups(Page::first())
            .await
            .unwrap()
            .data,
        [] as [loops_so::generated::GroupResponse; 0]
    );
    assert!(mock.called(Route::ListCampaignGroups).await);
}

#[tokio::test]
async fn get_campaign_group() {
    let (_mock, client) = setup().await;
    assert_eq!(
        client.get_campaign_group("mock-group-id").await.unwrap().id,
        "mock-group-id"
    );
}

#[tokio::test]
async fn create_campaign_group() {
    let (mock, client) = setup().await;
    let result = client
        .create_campaign_group("Group A", Some("desc"))
        .await
        .unwrap();
    assert_eq!(result.id, "mock-group-id");
    assert_eq!(
        body(&mock, Route::CreateCampaignGroup).await,
        json!({ "name": "Group A", "description": "desc" })
    );
}

#[tokio::test]
async fn create_campaign_group_error() {
    let (mock, client) = setup().await;
    mock.respond_json(
        Route::CreateCampaignGroup,
        400,
        json!({ "message": "Invalid group" }),
    );
    let err = client.create_campaign_group("bad", None).await.unwrap_err();
    assert_api_error(&err, "Invalid group", 400);
}

#[tokio::test]
async fn update_campaign_group() {
    let (mock, client) = setup().await;
    client
        .update_campaign_group("mock-group-id", Some("Group B"), None)
        .await
        .unwrap();
    assert_eq!(
        body(&mock, Route::UpdateCampaignGroup).await,
        json!({ "name": "Group B" })
    );
}

#[tokio::test]
async fn list_transactional_groups() {
    let (mock, client) = setup().await;
    assert_eq!(
        client
            .list_transactional_groups(Page::first())
            .await
            .unwrap()
            .data,
        [] as [loops_so::generated::GroupResponse; 0]
    );
    assert!(mock.called(Route::ListTransactionalGroups).await);
}

#[tokio::test]
async fn get_transactional_group() {
    let (_mock, client) = setup().await;
    assert_eq!(
        client
            .get_transactional_group("mock-group-id")
            .await
            .unwrap()
            .id,
        "mock-group-id"
    );
}

#[tokio::test]
async fn create_transactional_group() {
    let (mock, client) = setup().await;
    client
        .create_transactional_group("TGroup", None)
        .await
        .unwrap();
    assert_eq!(
        body(&mock, Route::CreateTransactionalGroup).await,
        json!({ "name": "TGroup" })
    );
}

#[tokio::test]
async fn update_transactional_group() {
    let (mock, client) = setup().await;
    client
        .update_transactional_group("mock-group-id", None, Some("new desc"))
        .await
        .unwrap();
    assert_eq!(
        body(&mock, Route::UpdateTransactionalGroup).await,
        json!({ "description": "new desc" })
    );
}

// ---------------------------------------------------------------------------
// A 400 on every route surfaces as an API error carrying the message
// ---------------------------------------------------------------------------

macro_rules! error_path_tests {
    ($($name:ident: $route:expr => |$client:ident| $call:expr;)+) => {
        $(
            #[tokio::test]
            async fn $name() {
                let (mock, $client) = setup().await;
                mock.respond_json($route, 400, json!({ "message": "boom" }));
                let err = $call.await.unwrap_err();
                assert_api_error(&err, "boom", 400);
            }
        )+
    };
}

error_path_tests! {
    error_list_campaigns: Route::ListCampaigns => |c| c.list_campaigns(Page::first());
    error_get_campaign: Route::GetCampaign => |c| c.get_campaign("x");
    error_create_campaign: Route::CreateCampaign => |c| c.create_campaign("x");
    error_update_campaign: Route::UpdateCampaign => |c| c.update_campaign("x", "y");
    error_list_components: Route::ListComponents => |c| c.list_components(Page::first());
    error_get_component: Route::GetComponent => |c| c.get_component("x");
    error_list_themes: Route::ListThemes => |c| c.list_themes(Page::first());
    error_get_theme: Route::GetTheme => |c| c.get_theme("x");
    error_get_email_message: Route::GetEmailMessage => |c| c.get_email_message("x");
    error_update_email_message: Route::UpdateEmailMessage => |c| c.update_email_message("x", EmailMessageUpdate::new().subject("x"));
    error_get_contact_suppression: Route::GetContactSuppression => |c| c.get_contact_suppression(ContactId::email("user@test.com"));
    error_remove_contact_suppression: Route::RemoveContactSuppression => |c| c.remove_contact_suppression(ContactId::email("user@test.com"));
    error_create_contact_property: Route::CreateContactProperty => |c| c.create_contact_property("x", ContactPropertyCreateRequestType::Number);
    error_list_transactional_emails: Route::ListTransactional => |c| c.list_transactional_emails(Page::first());
    error_list_transactional_templates: Route::ListTransactionalTemplates => |c| c.list_transactional_templates(Page::first());
    error_get_transactional_template: Route::GetTransactionalTemplate => |c| c.get_transactional_template("x");
    error_create_transactional_template: Route::CreateTransactionalTemplate => |c| c.create_transactional_template("x", None);
    error_update_transactional_template: Route::UpdateTransactionalTemplate => |c| c.update_transactional_template("x", Some("y"), None);
    error_draft_transactional_template: Route::DraftTransactionalTemplate => |c| c.draft_transactional_template("x");
    error_publish_transactional_template: Route::PublishTransactionalTemplate => |c| c.publish_transactional_template("x");
    error_create_upload: Route::CreateUpload => |c| c.create_upload("image/png", 1);
    error_get_workflow_node: Route::GetWorkflowNode => |c| c.get_workflow_node("wf", "node");
    error_list_workflows: Route::ListWorkflows => |c| c.list_workflows(Page::first());
    error_list_audience_segments: Route::ListAudienceSegments => |c| c.list_audience_segments(Page::first());
    error_get_audience_segment: Route::GetAudienceSegment => |c| c.get_audience_segment("x");
    error_list_campaign_groups: Route::ListCampaignGroups => |c| c.list_campaign_groups(Page::first());
    error_get_campaign_group: Route::GetCampaignGroup => |c| c.get_campaign_group("x");
    error_update_campaign_group: Route::UpdateCampaignGroup => |c| c.update_campaign_group("x", Some("y"), None);
    error_list_transactional_groups: Route::ListTransactionalGroups => |c| c.list_transactional_groups(Page::first());
    error_get_transactional_group: Route::GetTransactionalGroup => |c| c.get_transactional_group("x");
    error_create_transactional_group: Route::CreateTransactionalGroup => |c| c.create_transactional_group("x", None);
    error_update_transactional_group: Route::UpdateTransactionalGroup => |c| c.update_transactional_group("x", Some("y"), None);
    error_preview_email_message: Route::PreviewEmailMessage => |c| c.preview_email_message("x", &["user@test.com"], PreviewOptions::new());
    error_create_audience_segment: Route::CreateAudienceSegment => |c| c.create_audience_segment("x", json!({ "match": "all", "conditions": [] }), None);
    error_create_component: Route::CreateComponent => |c| c.create_component("x", "<Text>x</Text>");
    error_update_component: Route::UpdateComponent => |c| c.update_component("x", Some("y"), None);
    error_create_theme: Route::CreateTheme => |c| c.create_theme("x", None);
    error_update_theme: Route::UpdateTheme => |c| c.update_theme("x", Some("y"), None);
    error_get_email_message_guardian: Route::GetEmailMessageGuardian => |c| c.get_email_message_guardian("x");
    error_list_event_patterns: Route::ListEventPatterns => |c| c.list_event_patterns(Page::first());
    error_get_event_pattern: Route::GetEventPattern => |c| c.get_event_pattern("x");
    error_get_event_pattern_by_name: Route::GetEventPatternByName => |c| c.get_event_pattern_by_name("signup");
    error_create_workflow: Route::CreateWorkflow => |c| c.create_workflow("x", None, None);
    error_update_workflow: Route::UpdateWorkflow => |c| c.update_workflow("wf", Some("rev"), Some("y"), None);
    error_change_workflow_mailing_list: Route::ChangeWorkflowMailingList => |c| c.change_workflow_mailing_list("wf", Some("rev"), Some("list-id"), None, None);
    error_create_workflow_node: Route::CreateWorkflowNode => |c| c.create_workflow_node("wf", CreateWorkflowNodeTypeName::TimerAction, Some("rev"), NodePlacement::before("n1"));
    error_update_workflow_node: Route::UpdateWorkflowNode => |c| c.update_workflow_node("wf", "n1", Some("rev"), json!({ "typeName": "SignupTrigger" }));
    error_add_workflow_branch: Route::AddWorkflowBranch => |c| c.add_workflow_branch("wf", "n1", Some("rev"));
    error_reroute_node_connection: Route::RerouteNodeConnection => |c| c.reroute_workflow_node_connection("wf", "n1", Some("rev"), "n9");
    error_delete_workflow: Route::DeleteWorkflow => |c| c.delete_workflow("wf", Some("rev"), None);
    error_delete_workflow_node: Route::DeleteWorkflowNode => |c| c.delete_workflow_node("wf", "n1", Some("rev"), DeleteNodeOptions::new());
    error_delete_workflow_node_recursively: Route::DeleteWorkflowNodeRecursively => |c| c.delete_workflow_node("wf", "n1", Some("rev"), DeleteNodeOptions::new().recursive(true));
    error_get_campaign_metrics: Route::GetCampaignMetrics => |c| c.get_campaign_metrics("x");
    error_get_transactional_template_metrics: Route::GetTransactionalTemplateMetrics => |c| c.get_transactional_template_metrics("x");
    error_get_workflow_node_metrics: Route::GetWorkflowNodeMetrics => |c| c.get_workflow_node_metrics("wf", "n1");
    error_list_contact_properties: Route::ListContactProperties => |c| c.list_contact_properties();
    error_list_mailing_lists: Route::ListMailingLists => |c| c.list_mailing_lists();
    error_list_sending_ips: Route::ListSendingIps => |c| c.list_sending_ips();
    error_health: Route::Health => |c| c.health();
}

fn uuid_like(value: &str) -> bool {
    value.len() == 36 && value.chars().filter(|c| *c == '-').count() == 4
}
