//! Unofficial Rust SDK for [Loops.so](https://loops.so), the email platform
//! for SaaS.
//!
//! Two layers:
//!
//! - [`LoopsClient`] wraps every Loops endpoint with typed arguments, maps
//!   failures to [`LoopsError`], and enforces [safe mode](LoopsClientBuilder::safe_mode).
//!   This is what most code should use.
//! - [`generated`] is generated from the Loops OpenAPI spec by [oas3-gen]. It
//!   holds the request and response models the client returns, plus the raw
//!   `LoopsOpenApiSpecClient` (see [`LoopsClient::low_level`]).
//!
//! ```no_run
//! # async fn run() -> loops_so::Result<()> {
//! use loops_so::{ConfigureOptions, TransactionalOptions};
//!
//! // Or set the LOOPS_API_KEY environment variable.
//! loops_so::configure(ConfigureOptions::new().api_key("your_api_key"));
//! let client = loops_so::get_client()?;
//!
//! let variables = client.get_transactional_variables("clfq6dinn000yl70fgwwyp82l").await?;
//! client
//!     .send_transactional_email(
//!         "clfq6dinn000yl70fgwwyp82l",
//!         "user@example.com",
//!         TransactionalOptions::new()
//!             .data_variable("firstName", "Jane")
//!             .data_variable("inviteLink", "https://myapp.com/invite/abc123"),
//!     )
//!     .await?;
//! # let _ = variables;
//! # Ok(())
//! # }
//! ```
//!
//! [oas3-gen]: https://github.com/eklipse2k8/oas3-gen

mod client;
mod config;
mod error;
pub mod responses;
pub mod routes;
mod spec;
#[cfg(feature = "testing")]
pub mod testing;

/// Code generated from the Loops OpenAPI spec. Never edit by hand: `just
/// regenerate` replaces the whole directory.
///
/// Two oas3-gen limitations matter if you use these models directly:
///
/// - A decoded member of a discriminated union (a workflow node, an audience
///   filter condition) does not re-emit its tag (`typeName`, `type`) when
///   serialized again. Build request JSON yourself rather than re-sending a
///   decoded model.
/// - The low-level client decodes some failure bodies into untagged enums
///   that keep only the fields the first matching variant knows. The
///   high-level [`LoopsClient`] never does this: its [`LoopsError::Api`]
///   carries the raw JSON body.
#[allow(
    clippy::all,
    clippy::pedantic,
    missing_docs,
    rustdoc::bare_urls,
    rustdoc::broken_intra_doc_links,
    rustdoc::invalid_html_tags
)]
pub mod generated;

pub use client::{
    ContactFields, ContactId, DeleteNodeOptions, EmailFormat, EmailMessageUpdate, EventOptions,
    LoopsClient, LoopsClientBuilder, NodePlacement, Page, PreviewOptions, TransactionalOptions,
    get_client, reset_client,
};
pub use config::{
    API_KEY_ENV_VAR, ConfigureOptions, DEFAULT_BASE_URL, LoopsConfig, configure, get_config,
};
pub use error::{LoopsError, Result};
pub use responses::{
    Contact, TransactionalEmail, TransactionalEmailPagination, TransactionalEmailsResponse,
};
pub use spec::{LOOPS_API_VERSION, OPENAPI_SPEC, openapi_spec_path};
