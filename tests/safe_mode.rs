//! Safe mode: with it on, every recipient address must end in an allowed
//! domain or the call fails before anything is sent.

use std::sync::Arc;

use loops_so::routes::Route;
use loops_so::testing::LoopsMock;
use loops_so::{
    ConfigureOptions, ContactFields, ContactId, EmailMessageUpdate, EventOptions, LoopsClient,
    LoopsError, PreviewOptions, TransactionalOptions, configure, get_client, get_config,
    reset_client,
};

const ALLOWED_DOMAINS: [&str; 3] = ["@test.com", "@example.com", "@mycompany.io"];
const UNSAFE_EMAILS: [&str; 3] = ["user@gmail.com", "user@company.io", "user@loops.so"];
const ALLOWED_EMAILS: [&str; 4] = [
    "dev@test.com",
    "dev@example.com",
    "dev@mycompany.io",
    "DEV@MYCOMPANY.IO",
];

async fn safe_client() -> (LoopsMock, Arc<LoopsClient>) {
    let mock = LoopsMock::builder()
        .safe_mode(true)
        .safe_mode_allowed_domains(ALLOWED_DOMAINS)
        .start()
        .await;
    (mock, get_client().unwrap())
}

#[track_caller]
fn assert_unsafe<T: std::fmt::Debug>(result: loops_so::Result<T>, email: &str) {
    match result {
        Err(LoopsError::UnsafeEmail {
            email: rejected,
            allowed_domains,
        }) => {
            assert_eq!(rejected, email);
            assert_eq!(allowed_domains, ALLOWED_DOMAINS);
        }
        other => panic!("{email}: expected UnsafeEmail, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Blocked addresses never reach the API
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_transactional_email_blocked() {
    let (mock, client) = safe_client().await;
    for email in UNSAFE_EMAILS {
        assert_unsafe(
            client
                .send_transactional_email("tpl_1", email, TransactionalOptions::new())
                .await,
            email,
        );
    }
    assert!(!mock.called(Route::Transactional).await);
}

#[tokio::test]
async fn create_contact_blocked() {
    let (mock, client) = safe_client().await;
    for email in UNSAFE_EMAILS {
        assert_unsafe(
            client
                .create_contact(email, None, ContactFields::new())
                .await,
            email,
        );
    }
    assert!(!mock.called(Route::CreateContact).await);
}

#[tokio::test]
async fn upsert_contact_blocked() {
    let (mock, client) = safe_client().await;
    for email in UNSAFE_EMAILS {
        assert_unsafe(
            client
                .upsert_contact(ContactId::email(email), ContactFields::new())
                .await,
            email,
        );
        assert_unsafe(
            client
                .upsert_contact(ContactId::both(email, "user-1"), ContactFields::new())
                .await,
            email,
        );
    }
    assert!(!mock.called(Route::UpsertContact).await);
}

#[tokio::test]
async fn find_contact_blocked() {
    let (mock, client) = safe_client().await;
    for email in UNSAFE_EMAILS {
        assert_unsafe(client.find_contact(ContactId::email(email)).await, email);
    }
    assert!(!mock.called(Route::FindContact).await);
}

#[tokio::test]
async fn delete_contact_blocked() {
    let (mock, client) = safe_client().await;
    for email in UNSAFE_EMAILS {
        assert_unsafe(client.delete_contact(ContactId::email(email)).await, email);
    }
    assert!(!mock.called(Route::DeleteContact).await);
}

#[tokio::test]
async fn send_event_blocked() {
    let (mock, client) = safe_client().await;
    for email in UNSAFE_EMAILS {
        assert_unsafe(
            client
                .send_event("signup", ContactId::email(email), EventOptions::new())
                .await,
            email,
        );
    }
    assert!(!mock.called(Route::SendEvent).await);
}

#[tokio::test]
async fn contact_properties_cannot_smuggle_a_blocked_address() {
    let (mock, client) = safe_client().await;
    let err = client
        .send_event(
            "signup",
            ContactId::email("dev@test.com"),
            EventOptions::new().contact_property("email", "customer@gmail.com"),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, LoopsError::InvalidInput(_)), "{err:?}");
    let err = client
        .upsert_contact(
            ContactId::user_id("user-1"),
            ContactFields::new().property("email", "customer@gmail.com"),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, LoopsError::InvalidInput(_)), "{err:?}");
    assert!(mock.server().received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn update_email_message_cc_bcc_blocked() {
    let (mock, client) = safe_client().await;
    for email in UNSAFE_EMAILS {
        assert_unsafe(
            client
                .update_email_message(
                    "mock-email-message-id",
                    EmailMessageUpdate::new().cc_email(email),
                )
                .await,
            email,
        );
        assert_unsafe(
            client
                .update_email_message(
                    "mock-email-message-id",
                    EmailMessageUpdate::new().bcc_email(email),
                )
                .await,
            email,
        );
    }
    assert!(!mock.called(Route::UpdateEmailMessage).await);
}

#[tokio::test]
async fn preview_email_message_blocked() {
    let (mock, client) = safe_client().await;
    for email in UNSAFE_EMAILS {
        assert_unsafe(
            client
                .preview_email_message(
                    "mock-email-message-id",
                    &["dev@test.com", email],
                    PreviewOptions::new(),
                )
                .await,
            email,
        );
    }
    assert!(!mock.called(Route::PreviewEmailMessage).await);
}

// ---------------------------------------------------------------------------
// Allowed addresses go through, case-insensitively
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_transactional_email_allowed() {
    let (_mock, client) = safe_client().await;
    for email in ALLOWED_EMAILS {
        let result = client
            .send_transactional_email("tpl_1", email, TransactionalOptions::new())
            .await
            .unwrap_or_else(|err| panic!("{email}: {err}"));
        assert!(result.success);
    }
}

#[tokio::test]
async fn create_contact_allowed() {
    let (_mock, client) = safe_client().await;
    for email in ALLOWED_EMAILS {
        let result = client
            .create_contact(email, None, ContactFields::new())
            .await
            .unwrap_or_else(|err| panic!("{email}: {err}"));
        assert!(result.success);
    }
}

#[tokio::test]
async fn send_event_allowed() {
    let (_mock, client) = safe_client().await;
    for email in ALLOWED_EMAILS {
        let result = client
            .send_event("signup", ContactId::email(email), EventOptions::new())
            .await
            .unwrap_or_else(|err| panic!("{email}: {err}"));
        assert!(result.success);
    }
}

#[tokio::test]
async fn update_email_message_cc_bcc_allowed() {
    let (_mock, client) = safe_client().await;
    for email in ALLOWED_EMAILS {
        let result = client
            .update_email_message(
                "mock-email-message-id",
                EmailMessageUpdate::new().cc_email(email).bcc_email(email),
            )
            .await
            .unwrap_or_else(|err| panic!("{email}: {err}"));
        assert_eq!(result.id, "mock-email-message-id");
    }
}

#[tokio::test]
async fn allowed_domains_match_case_insensitively_both_ways() {
    let mock = LoopsMock::builder()
        .safe_mode(true)
        .safe_mode_allowed_domains(["@MyCompany.IO"])
        .start()
        .await;
    mock.client()
        .send_transactional_email("tpl_1", "dev@mycompany.io", TransactionalOptions::new())
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// No address, no check
// ---------------------------------------------------------------------------

#[tokio::test]
async fn upsert_by_user_id_skips_validation() {
    let (_mock, client) = safe_client().await;
    let result = client
        .upsert_contact(
            ContactId::user_id("usr_123"),
            ContactFields::new().first_name("Test"),
        )
        .await
        .unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn send_event_by_user_id_skips_validation() {
    let (_mock, client) = safe_client().await;
    let result = client
        .send_event("signup", ContactId::user_id("usr_123"), EventOptions::new())
        .await
        .unwrap();
    assert!(result.success);
}

// ---------------------------------------------------------------------------
// Safe mode off and configuration flow
// ---------------------------------------------------------------------------

#[tokio::test]
async fn safe_mode_off_allows_any_email() {
    let mock = LoopsMock::start().await;
    let client = LoopsClient::builder()
        .api_key("test-key")
        .base_url(mock.base_url())
        .safe_mode(false)
        .build()
        .unwrap();
    let result = client
        .send_transactional_email("tpl_1", "user@gmail.com", TransactionalOptions::new())
        .await
        .unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn configure_safe_mode_reflected_in_get_config() {
    let _mock = LoopsMock::start().await;
    configure(
        ConfigureOptions::new()
            .safe_mode(true)
            .safe_mode_allowed_domains(["@dev.com"]),
    );
    let config = get_config();
    assert!(config.safe_mode);
    assert_eq!(config.safe_mode_allowed_domains, ["@dev.com"]);
}

#[tokio::test]
async fn get_client_picks_up_configured_safe_mode() {
    let _mock = LoopsMock::builder()
        .safe_mode(true)
        .safe_mode_allowed_domains(["@test.com"])
        .start()
        .await;
    let client = get_client().unwrap();
    assert!(client.safe_mode());
    assert_unsafe_any(
        &client
            .send_transactional_email("tpl_1", "user@gmail.com", TransactionalOptions::new())
            .await,
    );
}

#[tokio::test]
async fn reset_client_picks_up_changed_safe_mode() {
    let _mock = LoopsMock::start().await;
    let client = get_client().unwrap();
    client
        .send_transactional_email("tpl_1", "user@gmail.com", TransactionalOptions::new())
        .await
        .unwrap();

    configure(
        ConfigureOptions::new()
            .safe_mode(true)
            .safe_mode_allowed_domains(["@test.com"]),
    );
    // The cached client keeps the settings it was built with...
    get_client()
        .unwrap()
        .send_transactional_email("tpl_1", "user@gmail.com", TransactionalOptions::new())
        .await
        .unwrap();
    // ...until the singleton is reset.
    reset_client();
    assert_unsafe_any(
        &get_client()
            .unwrap()
            .send_transactional_email("tpl_1", "user@gmail.com", TransactionalOptions::new())
            .await,
    );
}

#[tokio::test]
async fn mock_defaults_to_safe_mode_off() {
    let _mock = LoopsMock::start().await;
    let result = get_client()
        .unwrap()
        .send_transactional_email("tpl_1", "user@gmail.com", TransactionalOptions::new())
        .await
        .unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn explicit_safe_mode_overrides_config() {
    let mock = LoopsMock::start().await;
    let client = LoopsClient::builder()
        .api_key("test-key")
        .base_url(mock.base_url())
        .safe_mode(true)
        .safe_mode_allowed_domains(["@test.com"])
        .build()
        .unwrap();
    assert_unsafe_any(
        &client
            .send_transactional_email("tpl_1", "user@gmail.com", TransactionalOptions::new())
            .await,
    );
}

#[tokio::test]
async fn safe_mode_without_domains_is_a_configuration_error() {
    let _mock = LoopsMock::builder().safe_mode(true).start().await;
    let err = get_client()
        .unwrap()
        .send_transactional_email("tpl_1", "user@test.com", TransactionalOptions::new())
        .await
        .unwrap_err();
    assert!(matches!(err, LoopsError::Configuration(_)), "{err:?}");
    assert!(
        err.to_string().contains("no safe_mode_allowed_domains"),
        "{err}"
    );
}

#[tokio::test]
async fn custom_allowed_domains() {
    let _mock = LoopsMock::builder()
        .safe_mode(true)
        .safe_mode_allowed_domains(["@acme.dev"])
        .start()
        .await;
    let client = get_client().unwrap();
    client
        .send_transactional_email("tpl_1", "dev@acme.dev", TransactionalOptions::new())
        .await
        .unwrap();
    assert_unsafe_any(
        &client
            .send_transactional_email("tpl_1", "dev@test.com", TransactionalOptions::new())
            .await,
    );
}

#[track_caller]
fn assert_unsafe_any<T: std::fmt::Debug>(result: &loops_so::Result<T>) {
    assert!(
        matches!(result, Err(LoopsError::UnsafeEmail { .. })),
        "expected UnsafeEmail, got {result:?}"
    );
}
