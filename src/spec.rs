//! The Loops OpenAPI spec [`generated`](crate::generated) was built from.
//!
//! Consumers can check their own fakes or contract tests against exactly the
//! API this crate speaks.

use std::path::Path;

/// The `info.version` of the shipped spec. `just generate` rewrites it
/// together with the spec and the generated code.
pub const LOOPS_API_VERSION: &str = "1.22.1";

/// The shipped OpenAPI spec, as YAML.
pub const OPENAPI_SPEC: &str = include_str!("openapi.yaml");

/// Filesystem path of the shipped OpenAPI spec inside the crate sources.
pub fn openapi_spec_path() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src/openapi.yaml"))
}
