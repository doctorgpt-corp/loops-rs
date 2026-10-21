# loops-so

Unofficial Rust SDK for [Loops.so](https://loops.so).

[![crates.io](https://img.shields.io/crates/v/loops-so.svg)](https://crates.io/crates/loops-so)
[![docs.rs](https://img.shields.io/docsrs/loops-so)](https://docs.rs/loops-so)
[![Rust 1.99+](https://img.shields.io/badge/rust-1.99+-orange.svg)](https://blog.rust-lang.org/)

The Rust counterpart of [pyloops](https://github.com/doctorgpt-corp/pyloops): the
same two-layer design, safe mode, test mock and automated update flow.

## Installation

```bash
cargo add loops-so
```

The client is async and runs on [tokio](https://tokio.rs) through `reqwest`.
TLS uses rustls by default; enable `native-tls` instead with
`default-features = false, features = ["native-tls"]`.

## Quick Start

loops-so offers two ways to interact with the Loops API.

### High-Level API

```rust
use loops_so::{ConfigureOptions, ContactFields, ContactId, EventOptions, TransactionalOptions};

// Configure once (or set the LOOPS_API_KEY environment variable)
loops_so::configure(ConfigureOptions::new().api_key("your_api_key_here"));

// Get the client
let client = loops_so::get_client()?;

// Upsert a contact
client
    .upsert_contact(
        ContactId::email("user@example.com"),
        ContactFields::new().first_name("John").last_name("Doe").subscribed(true),
    )
    .await?;

// Find a contact
let contacts = client.find_contact(ContactId::email("user@example.com")).await?;

// Send an event
client
    .send_event(
        "user_signup",
        ContactId::email("user@example.com"),
        EventOptions::new().event_property("plan", "premium"),
    )
    .await?;

// List mailing lists
let mailing_lists = client.list_mailing_lists().await?;

// Look up the data variables a transactional template expects, then send it
let variables = client.get_transactional_variables("clfq6dinn000yl70fgwwyp82l").await?;
// -> ["firstName", "inviteLink"]

client
    .send_transactional_email(
        "clfq6dinn000yl70fgwwyp82l",
        "user@example.com",
        TransactionalOptions::new()
            .data_variable("firstName", "Jane")
            .data_variable("inviteLink", "https://myapp.com/invite/abc123"),
    )
    .await?;
```

Failures come back as [`LoopsError`](https://docs.rs/loops-so/latest/loops_so/enum.LoopsError.html):
`Api` (with the status and the message Loops sent), `RateLimit` (with the
`x-ratelimit-*` headers), `ContactExists`, `UnsafeEmail`, `InvalidInput`,
`Configuration`, `Http` and `Decode`.

### Low-Level API

For more control, use the client generated from the OpenAPI spec directly. It
shares the high-level client's base URL and API key:

```rust
use loops_so::generated::{ContactUpdateRequest, UpdateContactRequest};

let client = loops_so::LoopsClient::builder().api_key("your_api_key_here").build()?;
let low_level = client.low_level()?;

let response = low_level
    .update_contact(UpdateContactRequest {
        body: ContactUpdateRequest {
            email: Some("user@example.com".into()),
            first_name: Some("John".into()),
            ..Default::default()
        },
    })
    .await?;
```

The request and response models live in `loops_so::generated` and are what the
high-level methods return.

## Authentication

All API calls require a Loops API key. Get your API key from your [Loops account settings](https://app.loops.so/settings).

There are three ways to configure authentication:

1. **Environment variable**:
   ```bash
   export LOOPS_API_KEY="your_api_key_here"
   ```

2. **Process-wide configuration**:
   ```rust
   loops_so::configure(ConfigureOptions::new().api_key("your_api_key_here"));
   let client = loops_so::get_client()?;
   ```

3. **Per-client configuration**:
   ```rust
   let client = loops_so::LoopsClient::builder().api_key("your_api_key_here").build()?;
   ```

`LoopsClient::builder()` also takes a `base_url` and a preconfigured
`reqwest::Client` (for timeouts or a proxy). Every setting left unset falls back
to `configure()`.

## Features

The high-level `LoopsClient` wraps every Loops.so API endpoint:

- **Contacts**: Create, upsert, find, and delete contacts
- **Contact Properties**: List and create custom contact properties
- **Contact Suppression**: Read and remove suppression status
- **Mailing Lists**: View available mailing lists
- **Events**: Trigger event-based emails
- **Event Patterns**: List and look up triggerable event types
- **Transactional Emails**: Send, list, and manage transactional templates (create, update, draft, publish), and read the data variables a template expects
- **Campaigns & Campaign Groups**: List, get, create, and update
- **Transactional Groups**: List, get, create, and update
- **Workflow Groups**: List, get, create, and update
- **Themes & Components**: List, get, create, and update reusable branding/building blocks
- **Email Messages**: Get, update, preview, and run Guardian content checks
- **Workflows & Workflow Nodes**: Create, update, and delete workflows, change mailing lists, and build the node graph (create/update/branch/reroute/delete nodes)
- **Audience Segments**: List, get, and create
- **Uploads**: Create and complete asset uploads
- **Metrics**: Campaign, transactional template and workflow email node metrics
- **Sending IPs**: Retrieve dedicated sending IP addresses

## Safe Mode

When developing locally, you can enable **safe mode** to prevent accidentally sending emails or syncing contacts to real addresses. With safe mode enabled, only emails matching your allowed domains will be accepted — all others fail with `LoopsError::UnsafeEmail` before any request is sent. Safe mode checks the addresses a call carries: a call that identifies the contact only by user ID (e.g. `send_event` with `ContactId::user_id`) has no address to check and goes through, as in pyloops.

```rust
use loops_so::{ConfigureOptions, TransactionalOptions};

loops_so::configure(
    ConfigureOptions::new()
        .api_key("your_api_key_here")
        .safe_mode(true)
        .safe_mode_allowed_domains(["@test.com", "@example.com", "@yourcompany.com"]),
);

let client = loops_so::get_client()?;

// This works
client.send_transactional_email("welcome", "dev@test.com", TransactionalOptions::new()).await?;

// This fails with LoopsError::UnsafeEmail
let blocked = client
    .send_transactional_email("welcome", "real-user@gmail.com", TransactionalOptions::new())
    .await;
assert!(matches!(blocked, Err(loops_so::LoopsError::UnsafeEmail { .. })));
```

You can also set it per client:

```rust
let client = loops_so::LoopsClient::builder()
    .api_key("your_api_key_here")
    .safe_mode(true)
    .safe_mode_allowed_domains(["@test.com"])
    .build()?;
```

## Testing

loops-so ships a `testing` module that mocks every Loops API endpoint with a
local [wiremock](https://docs.rs/wiremock) server. Your real client code runs
end-to-end, but no actual API request leaves the process.

Enable the `testing` feature for your tests:

```toml
[dev-dependencies]
loops-so = { version = "0.1", features = ["testing"] }
```

### Basic usage

```rust
use loops_so::routes::Route;
use loops_so::testing::LoopsMock;
use loops_so::TransactionalOptions;

#[tokio::test]
async fn sends_welcome_email() {
    let mock = LoopsMock::start().await;
    let client = loops_so::get_client().unwrap();
    client
        .send_transactional_email(
            "welcome",
            "user@test.com",
            TransactionalOptions::new().data_variable("name", "Jan"),
        )
        .await
        .unwrap();

    // Inspect the HTTP request that loops-so made
    let request = &mock.calls(Route::Transactional).await[0];
    let body: serde_json::Value = request.body_json().unwrap();
    assert_eq!(body["transactionalId"], "welcome");
    assert_eq!(body["email"], "user@test.com");
}
```

`LoopsMock::start()` configures the process-wide client to use the mock, resets
the `get_client()` singleton, and holds a lock so tests using it run one at a
time. Dropping it resets the singleton and restores the previous configuration.
`mock.client()` gives a client bound to the mock without touching the singleton.

### Simulating errors

Override any route to return custom responses:

```rust
use loops_so::testing::wiremock::ResponseTemplate;

#[tokio::test]
async fn handles_rate_limit() {
    let mock = LoopsMock::start().await;
    mock.respond(
        Route::Transactional,
        ResponseTemplate::new(429)
            .insert_header("x-ratelimit-limit", "10")
            .insert_header("x-ratelimit-remaining", "0"),
    );
    let client = loops_so::get_client().unwrap();
    let err = client
        .send_transactional_email("abc", "user@test.com", TransactionalOptions::new())
        .await
        .unwrap_err();
    assert!(matches!(err, loops_so::LoopsError::RateLimit { limit: 10, remaining: 0 }));
}
```

`mock.respond_json(route, status, body)` is a shorthand for JSON bodies; the
latest override of a route wins and `mock.reset_route(route)` restores the
default. A request to an unmocked path gets a 501 naming the method and path.

### Available mock routes

Every Loops endpoint the client supports has a matching `Route`, named after
the client method that calls it, so a call to `client.create_workflow(...)` is
recorded under `Route::CreateWorkflow`. `Route::from_name("create_workflow")`
looks one up by its snake-case name. Common routes:

| Route | Method | Endpoint |
|-------|--------|----------|
| `Health` | GET | `/v1/api-key` |
| `Transactional` | POST | `/v1/transactional` |
| `ListTransactional` | GET | `/v1/transactional` |
| `CreateContact` | POST | `/v1/contacts/create` |
| `UpsertContact` | PUT | `/v1/contacts/update` |
| `FindContact` | GET | `/v1/contacts/find` |
| `DeleteContact` | POST | `/v1/contacts/delete` |
| `ListContactProperties` | GET | `/v1/contacts/properties` |
| `CreateContactProperty` | POST | `/v1/contacts/properties` |
| `SendEvent` | POST | `/v1/events/send` |
| `ListMailingLists` | GET | `/v1/lists` |
| `ListSendingIps` | GET | `/v1/dedicated-sending-ips` |

Additional families are mocked too — campaigns, campaign/transactional/workflow groups,
transactional templates, themes, components, email messages (incl. preview and
Guardian), uploads, audience segments, event patterns, metrics, and
workflows/workflow nodes. See `Route::ALL` for the full list.

### Testing with safe mode

The mock disables safe mode by default. To test safe mode behavior, turn it on with your allowed domains:

```rust
let _mock = LoopsMock::builder()
    .safe_mode(true)
    .safe_mode_allowed_domains(["@test.com"])
    .start()
    .await;
```

## Documentation

API reference: [docs.rs/loops-so](https://docs.rs/loops-so). For the Loops API
itself, see the [Loops.so API docs](https://loops.so/docs).

## Automated Updates

This SDK is automatically updated to match the latest Loops.so API specification (current: **1.22.6**, also exposed as `loops_so::LOOPS_API_VERSION`).

The crate follows semver below 1.0: a release built on a new Loops API version
bumps the minor version (regenerated models usually change shape, which is
breaking in Rust), as does a breaking change to the hand-written client. Other
releases bump the patch version. Depend on `loops-so = "0.x"` to get compatible
fixes without picking up a new API version unannounced.

A GitHub Action checks for API updates daily and opens a pull request when changes are detected. After review and merge, a new version is automatically published to crates.io. A merge that leaves the shipped code untouched - docs or tests only - publishes nothing.

## Development

### Setup

```bash
git clone https://github.com/doctorgpt-corp/loops-rs.git
cd loops-rs

# The pinned toolchain (rust-toolchain.toml) installs on first use.
# Install the pinned code generator:
just install
```

### Running checks

```bash
just check      # fmt + clippy + tests + docs + feature matrix
just lint       # fmt --check + clippy -D warnings
just test       # tests only
just format     # rustfmt + clippy --fix
```

### Project Structure

```
src/
├── lib.rs               # Public exports
├── client/              # High-level LoopsClient, one file per endpoint family
├── config.rs            # configure() / get_config()
├── error.rs             # LoopsError
├── responses.rs         # Hand-written response models
├── routes.rs            # The named route table (client, mock, spec test)
├── spec.rs              # LOOPS_API_VERSION, OPENAPI_SPEC, openapi_spec_path()
├── testing.rs           # LoopsMock (feature "testing")
├── openapi.yaml         # The Loops OpenAPI spec generated/ was built from
└── generated/           # ALL auto-generated code (oas3-gen)
    ├── mod.rs
    ├── client.rs
    └── types.rs
xtask/                   # Spec normalizer run before the generator
```

### Regenerate SDK

To fetch the latest OpenAPI spec, regenerate, and run all checks:

```bash
just generate
```

To regenerate from the committed `src/openapi.yaml` only:

```bash
just regenerate
```

Regeneration runs `cargo xtask normalize-spec` to work around oas3-gen
limitations (OpenAPI 3.1 nullable types, implicit discriminator mappings, and
closed objects that would reject fields Loops adds), then the pinned
`oas3-gen`, then `cargo fmt`. Custom code is never touched, and CI fails if `src/generated/`
differs from what the generator produces.

### The spec ships with the crate

The Loops OpenAPI spec the client was generated from is committed as
`src/openapi.yaml` and included in the published crate, so a consumer can check
its own fakes or contract tests against exactly the API the installed client
speaks:

```rust
let yaml: &str = loops_so::OPENAPI_SPEC;
let path: &std::path::Path = loops_so::openapi_spec_path();
```

## License

MIT

## Disclaimer

This is an unofficial SDK and is not affiliated with or endorsed by Loops.so.
