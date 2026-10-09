# The oas3-gen release that writes src/generated/. It must match the exact
# `oas3-gen-support` pin in Cargo.toml; tests/spec.rs fails when they drift.
oas3_gen_version := "0.28.0"
spec_url := "https://app.loops.so/openapi.yaml"

# Run all checks
default: check

# Install the pinned code generator
install:
    cargo install oas3-gen --version {{oas3_gen_version}} --locked

# Check formatting and lints
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

# Format code and apply lint fixes
format:
    cargo fmt --all
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged

# Run the test suite
test:
    cargo test --workspace --all-features --locked

# Build the docs with warnings as errors
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --locked

# Check the build with the alternative TLS backend
features:
    cargo check --no-default-features --features native-tls --locked

# Run all checks (lint + tests + docs + feature matrix)
check: lint test doc features

# Package the crate as crates.io would receive it
build:
    cargo package --locked --allow-dirty

# Fetch the latest Loops OpenAPI spec, regenerate, and run all checks
generate: fetch regenerate check

# Download the latest Loops OpenAPI spec into src/openapi.yaml
fetch:
    curl -fsSL {{spec_url}} -o src/openapi.yaml

# Regenerate src/generated/ from the committed src/openapi.yaml
regenerate:
    #!/usr/bin/env bash
    set -euo pipefail
    installed=$(oas3-gen --version 2>/dev/null | awk '{print $2}' || true)
    if [ "$installed" != "{{oas3_gen_version}}" ]; then
        echo "oas3-gen {{oas3_gen_version}} is required (found: ${installed:-none}); run: just install" >&2
        exit 1
    fi
    mkdir -p target
    cargo xtask normalize-spec src/openapi.yaml target/openapi.normalized.json
    rm -rf src/generated
    oas3-gen generate client-mod --no-helpers --quiet \
        --input target/openapi.normalized.json --output src/generated
    version=$(grep -m1 '^  version:' src/openapi.yaml | awk '{print $2}')
    perl -pi -e "s/^pub const LOOPS_API_VERSION: &str = \".*\";/pub const LOOPS_API_VERSION: &str = \"$version\";/" src/spec.rs
    perl -pi -e "s/current: \*\*[0-9.]*\*\*/current: **$version**/" README.md
    cargo fmt --all
    echo "Regenerated src/generated/ from Loops API $version"

# Remove build artifacts
clean:
    cargo clean
