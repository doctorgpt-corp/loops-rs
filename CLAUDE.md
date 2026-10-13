# loops-rs

Unofficial Rust SDK for [Loops.so](https://loops.so) — an email marketing and transactional email platform. The library wraps the Loops REST API and is published to crates.io as `loops-so` (imported as `loops_so`). It is the Rust counterpart of [pyloops](https://github.com/doctorgpt-corp/pyloops) and keeps the same design and flows.

## Architecture

Two layers:

**High-level API** (`src/client/`) — `LoopsClient` wraps every Loops endpoint with typed arguments, maps failures to `LoopsError`, and enforces safe mode. This is what consumers use. `get_client()` / `reset_client()` manage a process-wide `Arc<LoopsClient>` built from `configure()`. One file per endpoint family; `src/client/mod.rs` holds construction, dispatch and the singleton, `src/client/params.rs` the argument types (`ContactId`, `ContactFields`, `Page`, ...).

**Low-level API** (`src/generated/`) — generated from the Loops OpenAPI spec by [oas3-gen](https://github.com/eklipse2k8/oas3-gen): request/response models (`types.rs`) and the raw `LoopsOpenApiSpecClient` (`client.rs`). Never edit this directory by hand; `just regenerate` replaces it wholesale. It is exempt from clippy and missing-docs via the attributes on `pub mod generated` in `lib.rs`.

The high-level client does **not** call `LoopsOpenApiSpecClient`. That client discards the status and headers of any response its spec does not list, so a 429 would lose its `x-ratelimit-*` headers. `LoopsClient::send` dispatches the request itself (URL from `routes.rs`), turns 429 into `LoopsError::RateLimit`, and hands 2xx responses to the generated `XRequest::parse_response` for typed decoding. Non-2xx responses become `LoopsError::Api` with the `message` from the JSON body.

Supporting modules:
- `src/routes.rs` - the named `Route` table (name, method, path template). The client builds URLs from it, the mock mounts one route per entry, and `tests/spec.rs` checks it against the spec in both directions.
- `src/config.rs` - process-wide config (`configure()`, `get_config()`), falls back to the `LOOPS_API_KEY` env var
- `src/error.rs` - `LoopsError` (`Api`, `RateLimit`, `ContactExists`, `UnsafeEmail`, `InvalidInput`, `Configuration`, `Http`, `Decode`) with `status_code()` and `response_body()`
- `src/responses.rs` - hand-written response models where the generated one falls short: `TransactionalEmailsResponse`, and `Contact`, whose spec schema omits the custom properties Loops returns
- `src/testing.rs` - `LoopsMock` (feature `testing`, see below)
- `src/openapi.yaml` - the Loops OpenAPI spec `src/generated/` was built from, shipped in the crate byte-for-byte as fetched; `just fetch` and `check-updates.yml` rewrite it, never by hand
- `src/spec.rs` - `LOOPS_API_VERSION` (rewritten by `just regenerate`), `OPENAPI_SPEC` (the embedded YAML) and `openapi_spec_path()`
- `xtask/` - `cargo xtask normalize-spec`, which rewrites the spec into the generator's input (see below). Not published.

Where the generated model cannot express a request (oneOf bodies such as `send_event` and `delete_contact`, string-only maps, required-but-nullable `expectedRevisionId`), the method builds the JSON body itself. The workflow node reads and mutations return `serde_json::Value`, as in pyloops, because the generated node unions drop the node fields.

## Spec normalization

oas3-gen mishandles four patterns in the Loops OpenAPI 3.1 spec, each of which made real responses decode wrongly or not at all:

1. A `$ref` to a schema whose `type` includes `"null"` (e.g. `AudienceFilter`) was generated as a non-optional field.
2. An inline enum with `type: [string, "null"]` and a `"null"` entry became a variant matching the *string* `"null"`.
3. `additionalProperties: false` became `#[serde(deny_unknown_fields)]`, so any field Loops adds before the spec catches up broke decoding, and discriminated members rejected their own tag.
4. A `oneOf` + `discriminator` without a `mapping` became an untagged enum whose first variant swallowed every member (every workflow node decoded as `SignupTrigger`).

`cargo xtask normalize-spec src/openapi.yaml target/openapi.normalized.json` rewrites (1) and (2) into `anyOf: [{$ref}, {type: "null"}]` (hoisting such enums into named schemas), drops (3), and derives the explicit mapping for (4) from each member's single-value enum. It prints what it changed; read that output on every regeneration. If it reports a new pattern or fails (a name clash, a union member without a single tag), fix the normalizer with a unit test in `xtask/src/main.rs`, never the generated code.

Known limitations the normalizer cannot reach (they are serde attributes the
generator emits, documented on `loops_so::generated`):

- A decoded discriminated-union member does not re-emit its tag when
  serialized (`#[serde(default, skip_deserializing)]` leaves it `None`).
  `tests/spec.rs::decoded_union_members_still_drop_their_tag_on_reserialize`
  fails once a generator bump fixes it; then remove the caveat.
- Some failure-body enums are untagged with overlapping variants. Only the
  low-level client decodes failure bodies with them; `LoopsClient` keeps the
  raw JSON in `LoopsError::Api`.

## Git conventions

Commit subjects **and PR titles** follow [Conventional Commits](https://www.conventionalcommits.org/):
`type(optional-scope): summary`, imperative mood, no trailing period.

| Type | Use for |
|---|---|
| `feat` | A new client method, endpoint, or user-visible capability |
| `fix` | A bug fix in the hand-written layer |
| `chore` | Regenerating `src/generated/`, dependency changes |
| `refactor` | Restructuring that does not change behaviour |
| `docs` | README, CLAUDE.md, rustdoc |
| `ci` | Anything under `.github/workflows/` |
| `test` | Test-only changes |
| `build` | `justfile`, `xtask/`, toolchain |

Append `!` before the colon when the change breaks consumers (`feat!:`,
`chore!:`) and explain the break in the body. On this SDK, breaking means
anything that stops a downstream crate compiling or changes what a call does: a
changed return or argument type, a removed or renamed public method, type,
field or variant, or a new required field on a public struct without
`#[non_exhaustive]`. A new method, a new builder method, or a new field on a
`#[non_exhaustive]` struct is non-breaking. `publish.yml` reads the `!` (and
`BREAKING CHANGE:` footers) to decide the version bump, so getting it right
matters.

Keep the subject under ~72 characters and put the reasoning in the body — why
the change is needed, not a restatement of the diff.

## Versioning

crates.io only accepts three-part semver, so pyloops' fourth version segment
does not carry over. The crate is `0.MINOR.PATCH` and the version in
`Cargo.toml` is not maintained by hand: on merge to main, `publish.yml`
computes the release from the git tags and stamps it into the runner's
checkout only.

- No tag yet: the `Cargo.toml` version (`0.1.0`).
- `src/openapi.yaml` `info.version` differs from the one at the last tag, or a
  commit since then is marked breaking: bump MINOR. Cargo treats a 0.x minor as
  incompatible, so consumers never pick up a regeneration unannounced.
- Otherwise: bump PATCH.

It releases only when `src/`, `Cargo.toml` or `Cargo.lock` changed since the
last tag, so a docs-, test-, `xtask/`- or CI-only merge ships nothing. The
Loops API version a release was built from is `LOOPS_API_VERSION`, the README
"current" line, the release notes, and the attached `openapi.<api>.yml`.

Publishing uses crates.io trusted publishing (`rust-lang/crates-io-auth-action`).
That only works for a crate that already exists, so the very first release is
bootstrapped by hand: `cargo publish` locally, configure the trusted publisher
on crates.io (repository `doctorgpt-corp/loops-rs`, workflow `publish.yml`),
then push. The workflow sees the version already on crates.io, skips the upload
and creates the tag and release.

## SDK version bump workflow

The procedure lives in the `sdk-update` skill
(`.claude/skills/sdk-update/SKILL.md`): triaging the bot PRs, telling generator
churn from a real spec change, and the repair recipe for each pattern. Run it
whenever a new Loops API version lands. These rules bind whatever route the
change takes:

- `src/generated/` is never hand-edited. `just regenerate` replaces it
  wholesale, and the `generated` CI job fails on any drift.
- The generator is pinned in three places that must agree: `oas3_gen_version`
  in the `justfile`, the exact `oas3-gen-support = "=x.y.z"` pin in
  `Cargo.toml`, and the version in the `src/generated/mod.rs` header.
  `tests/spec.rs::generator_versions_agree` enforces it. Bumping the generator
  is a deliberate `chore:` commit that regenerates in the same commit.
- A new client method is a four-part unit landing in one commit: the method in
  `src/client/`, a `Route` entry in `src/routes.rs` plus its default fixture in
  `testing::default_response`, the `EXPECTED_ROUTE_NAMES` entry with
  happy-path and error tests in `tests/testing.rs`, and a row in the
  named-route table below. `tests/spec.rs::every_spec_operation_has_a_route`
  fails until a new spec endpoint has all of this.

## Testing strategy

Tests need no real API key: every request goes to a local wiremock server.

```
tests/
  testing.rs     # 199 tests — mock utility + every client method (happy path, body, 400/404/409/429, decode)
  safe_mode.rs   # 24 tests  — safe mode email domain validation
  spec.rs        # 8 tests   - spec <-> generated client <-> Route table agreement, API version, generator pins
src/             # 3 unit tests — base_url normalization, route matching, mock drop restores config
xtask/           # 7 unit tests — spec normalizer
```

Run all checks (what CI runs):
```bash
just check     # fmt --check, clippy -D warnings (pedantic), tests, docs -D warnings, native-tls build
```

Format code (run before committing):
```bash
just format
```

The toolchain is pinned to Rust 1.99.0 in `rust-toolchain.toml`; `rust-version`
in `Cargo.toml` is the same MSRV.

### tests/testing.rs

Covers every public `LoopsClient` method:
- **Happy path** — every method against the mock's default response, result type and key fields asserted. The default fixtures double as a check that the generated models decode spec-shaped bodies.
- **Request body inspection** — exact JSON equality on what was sent: camelCase keys, unset fields omitted, `expectedRevisionId: null` kept, query strings, percent-encoded path parameters, idempotency headers
- **Error simulation** — a 400 on every route surfaces as `LoopsError::Api` carrying the message (`error_path_tests!`); 404/409 special cases; 429 asserts `LoopsError::RateLimit` with `limit`/`remaining`; malformed 2xx bodies are `LoopsError::Decode`
- **Input validation** — arguments rejected before any request: `ContactId::Both` where Loops takes one identifier, custom properties that would overwrite a field the request sets (e.g. `email`, which would bypass safe mode), empty or dot-only path parameters, long idempotency keys
- **Decoding** — populated workflow graphs decode each node by `typeName`, unknown response fields are tolerated, custom contact properties survive in `Contact::properties`
- **Mock lifecycle** — singleton reset, custom API key in the `Authorization` header, unmatched paths answered with 501, latest override wins however many there are
- **Named route completeness** — `EXPECTED_ROUTE_NAMES` must equal `Route::ALL`

When adding a new client method, add: one happy-path test with a body assertion, an `error_path_tests!` entry, and a 429 test if the endpoint is frequently called.

### tests/safe_mode.rs

With safe mode on, every address-taking method fails with `LoopsError::UnsafeEmail` before sending (blocked domains `@gmail.com`, `@company.io`, `@loops.so`), allowed domains pass case-insensitively, `ContactId::UserId` skips the check, and the `configure()` / `get_client()` / `reset_client()` flow picks up changes only after a reset.

## Testing module (`loops_so::testing`)

Enable the feature: `loops-so = { version = "0.1", features = ["testing"] }` (adds `wiremock`, re-exported as `loops_so::testing::wiremock`).

### Basic usage

```rust
use loops_so::routes::Route;
use loops_so::testing::LoopsMock;
use loops_so::TransactionalOptions;

#[tokio::test]
async fn sends() {
    let mock = LoopsMock::start().await;
    let client = loops_so::get_client().unwrap();
    let result = client
        .send_transactional_email("tpl_abc", "user@test.com", TransactionalOptions::new())
        .await
        .unwrap();
    assert!(result.success);

    // Inspect what loops-so actually sent over the wire
    let body: serde_json::Value = mock.calls(Route::Transactional).await[0].body_json().unwrap();
    assert_eq!(body["transactionalId"], "tpl_abc");
}
```

### What it does

`LoopsMock::start()` (or `LoopsMock::builder()...start()`):
1. Takes a process-wide async lock, so tests using the mock run one at a time under the parallel test runner
2. Starts a wiremock server and mounts a default success response for every `Route`, under `/api` to mirror `https://app.loops.so/api`, plus a lowest-priority catch-all answering 501
3. Calls `configure()` with the mock's base URL, API key (default `"test-key"`) and safe-mode settings, and resets the singleton
4. On drop, resets the singleton and restores the configuration it replaced

`mock.calls(route)`, `mock.called(route)`, `mock.respond(route, ResponseTemplate)` / `mock.respond_json(route, status, json)` (the latest override of a route wins), `mock.reset_route(route)` and `mock.client()` (a client bound to the mock, independent of the singleton).

### Named routes

| Name | Endpoint |
|---|---|
| `health` | `GET /v1/api-key` |
| `create_contact` | `POST /v1/contacts/create` |
| `upsert_contact` | `PUT /v1/contacts/update` |
| `find_contact` | `GET /v1/contacts/find` |
| `delete_contact` | `POST /v1/contacts/delete` |
| `list_contact_properties` | `GET /v1/contacts/properties` |
| `create_contact_property` | `POST /v1/contacts/properties` |
| `get_contact_suppression` | `GET /v1/contacts/suppression` |
| `remove_contact_suppression` | `DELETE /v1/contacts/suppression` |
| `send_event` | `POST /v1/events/send` |
| `list_mailing_lists` | `GET /v1/lists` |
| `transactional` | `POST /v1/transactional` |
| `list_transactional` | `GET /v1/transactional` |
| `list_sending_ips` | `GET /v1/dedicated-sending-ips` |
| `list_campaigns` | `GET /v1/campaigns` |
| `create_campaign` | `POST /v1/campaigns` |
| `get_campaign` | `GET /v1/campaigns/{id}` |
| `update_campaign` | `POST /v1/campaigns/{id}` |
| `list_components` | `GET /v1/components` |
| `get_component` | `GET /v1/components/{id}` |
| `list_themes` | `GET /v1/themes` |
| `get_theme` | `GET /v1/themes/{id}` |
| `get_email_message` | `GET /v1/email-messages/{id}` |
| `update_email_message` | `POST /v1/email-messages/{id}` |
| `list_transactional_templates` | `GET /v1/transactional-emails` |
| `get_transactional_template` | `GET /v1/transactional-emails/{id}` |
| `create_transactional_template` | `POST /v1/transactional-emails` |
| `update_transactional_template` | `POST /v1/transactional-emails/{id}` |
| `draft_transactional_template` | `POST /v1/transactional-emails/{id}/draft` |
| `publish_transactional_template` | `POST /v1/transactional-emails/{id}/publish` |
| `create_upload` | `POST /v1/uploads` |
| `complete_upload` | `POST /v1/uploads/{id}/complete` |
| `list_workflows` | `GET /v1/workflows` |
| `get_workflow` | `GET /v1/workflows/{id}` |
| `get_workflow_node` | `GET /v1/workflows/{id}/nodes/{node_id}` |
| `list_audience_segments` | `GET /v1/audience-segments` |
| `get_audience_segment` | `GET /v1/audience-segments/{id}` |
| `list_campaign_groups` | `GET /v1/campaign-groups` |
| `get_campaign_group` | `GET /v1/campaign-groups/{id}` |
| `create_campaign_group` | `POST /v1/campaign-groups` |
| `update_campaign_group` | `POST /v1/campaign-groups/{id}` |
| `list_transactional_groups` | `GET /v1/transactional-groups` |
| `get_transactional_group` | `GET /v1/transactional-groups/{id}` |
| `create_transactional_group` | `POST /v1/transactional-groups` |
| `update_transactional_group` | `POST /v1/transactional-groups/{id}` |
| `preview_email_message` | `POST /v1/email-messages/{id}/preview` |
| `create_audience_segment` | `POST /v1/audience-segments` |
| `create_component` | `POST /v1/components` |
| `update_component` | `POST /v1/components/{id}` |
| `create_theme` | `POST /v1/themes` |
| `update_theme` | `POST /v1/themes/{id}` |
| `get_email_message_guardian` | `GET /v1/email-messages/{id}/guardian` |
| `list_event_patterns` | `GET /v1/event-patterns` |
| `get_event_pattern` | `GET /v1/event-patterns/{id}` |
| `get_event_pattern_by_name` | `GET /v1/event-patterns/by-name/{name}` |
| `create_workflow` | `POST /v1/workflows` |
| `update_workflow` | `POST /v1/workflows/{id}` |
| `change_workflow_mailing_list` | `POST /v1/workflows/{id}/mailing-list` |
| `delete_workflow` | `DELETE /v1/workflows/{id}` |
| `create_workflow_node` | `POST /v1/workflows/{id}/nodes` |
| `update_workflow_node` | `POST /v1/workflows/{id}/nodes/{node_id}` |
| `add_workflow_branch` | `POST /v1/workflows/{id}/nodes/{node_id}/add-branch` |
| `reroute_node_connection` | `POST /v1/workflows/{id}/nodes/{node_id}/reroute` |
| `delete_workflow_node` | `DELETE /v1/workflows/{id}/nodes/{node_id}` |
| `delete_workflow_node_recursively` | `DELETE /v1/workflows/{id}/nodes/{node_id}/recursive` |
| `get_campaign_metrics` | `GET /v1/campaigns/{id}/metrics` |
| `get_transactional_template_metrics` | `GET /v1/transactional-emails/{id}/metrics` |
| `get_workflow_node_metrics` | `GET /v1/workflows/{id}/nodes/{node_id}/metrics` |
| `list_workflow_groups` | `GET /v1/workflow-groups` |
| `get_workflow_group` | `GET /v1/workflow-groups/{id}` |
| `create_workflow_group` | `POST /v1/workflow-groups` |
| `update_workflow_group` | `POST /v1/workflow-groups/{id}` |

The `Route` variant is the PascalCase name (`Route::Transactional`, `Route::DeleteWorkflowNodeRecursively`); `Route::from_name("transactional")` looks one up by name.

### Simulating errors

Override any route before calling the client; the latest override wins and `reset_route` restores the default:

```rust
use loops_so::testing::wiremock::ResponseTemplate;

mock.respond(
    Route::Transactional,
    ResponseTemplate::new(429)
        .insert_header("x-ratelimit-limit", "10")
        .insert_header("x-ratelimit-remaining", "0"),
);
let err = client
    .send_transactional_email("x", "u@test.com", TransactionalOptions::new())
    .await
    .unwrap_err();
assert!(matches!(err, LoopsError::RateLimit { limit: 10, remaining: 0 }));
```

### Safe mode in tests

```rust
let mock = LoopsMock::builder()
    .safe_mode(true)
    .safe_mode_allowed_domains(["@mycompany.com"])
    .start()
    .await;
// addresses not ending in @mycompany.com fail with LoopsError::UnsafeEmail
```
