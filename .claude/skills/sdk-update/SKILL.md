---
name: sdk-update
description: Handle an automated "chore: update SDK to Loops API version X" bot PR - triage the open ones, classify the regenerated diff, repair the hand-written client, and ship it. Use for "handle the bot SDK update PR", "new Loops API version", "update SDK to 1.x.y", or "close superseded update PRs".
---

# SDK update

The bot fetches the new spec and runs `just regenerate`, which rewrites
`src/generated/`, `LOOPS_API_VERSION` and the README version. It never touches
the hand-written client, so its PR usually cannot merge on its own: the crate
stops compiling on a model the generator renamed, or `tests/spec.rs` fails on a
new endpoint. Take the bot's branch, repair the client on top of it, ship both
together.

This repo is public and stands alone. Never name a consumer project, internal
service, host or secret store in code, comments, commit messages or PR text.

## 1. Triage the open bot PRs

```bash
gh pr list --state open --search "update SDK to Loops API version in:title"
```

Keep the newest API version. Close each older one:

```bash
gh pr comment <N> --body "Superseded by #<newest> (Loops API <x.y.z>)."
gh pr close <N> --delete-branch
```

Bot PRs are opened with `GITHUB_TOKEN`, so no workflow runs on them. An absent
or green check list means nothing - `just check` on your machine is the gate.

## 2. Branch

```bash
git fetch origin
git checkout -b sdk-<x.y.z> origin/update-sdk-<x.y.z>
just install   # the pinned oas3-gen, if not installed
```

The bot's regen commit stays first and untouched; repair commits go on top.
The tree may not compile between the regen and the first repair, which is
expected - say so in the PR body and ask for squash or merge, not rebase.

Re-run `just regenerate` once and confirm it leaves no diff: the bot and your
machine must agree, or the `generated` CI job will fail after merge.

## 3. Classify the diff

Read the normalizer's report first (`cargo xtask normalize-spec src/openapi.yaml
target/openapi.normalized.json`). A new hoisted enum, nullable schema, opened
object or derived discriminator mapping is expected churn; an error means a
pattern the normalizer must learn (fix it in
`xtask/` with a unit test, never in `src/generated/`).

Then hunt for the substantive changes:

| Looking for | Command |
|---|---|
| New or removed endpoints | `cargo test --test spec` (names them) |
| Every compile break in the client | `cargo check --all-features 2>&1 \| grep -E '^error'` |
| Renamed or removed models | `git diff origin/main..HEAD -- src/generated/types.rs \| grep -E '^[-+]pub (struct\|enum\|type) '` |
| Response enum variants that moved | `git diff origin/main..HEAD -- src/generated/types.rs \| grep -E '^[-+]    (Ok\|Created\|NoContent)'` |
| Bodies degraded to raw JSON | `git diff origin/main..HEAD -- src/generated/types.rs \| grep -E '^\+.*Variant[0-9]\(serde_json::Value\)'` |
| New required response fields | `git diff origin/main..HEAD -- src/generated/types.rs \| grep -E '^\+    pub [a-z_]+: [^O]'` |

A response field without `Option<...>` is required; the default fixtures in
`src/testing.rs` must carry it or the happy-path tests fail with
`LoopsError::Decode`.

## 4. Repair recipes

- **Model renamed** (the generator dedupes identical schemas, so names shift)
  - mechanical import and match-arm fix in `src/client/`. Own commit,
  `refactor:`, no behaviour change - unless the type is part of a public
  signature, in which case it is breaking: `refactor!:`.
- **Request body became a oneOf** (`Variant0(serde_json::Value)`) - build the
  JSON in the method with camelCase keys, send exactly the keys the caller
  supplied, and make invalid combinations unrepresentable in the argument types
  (an enum like `ContactId` or `NodePlacement`) rather than letting the API
  answer 400.
- **Success response became a union the generator flattens badly** - check
  what the generated type keeps against the spec. If it drops fields, return
  `serde_json::Value` via `RawResponse::parse_json`, as the workflow node
  methods do. The return type changes: breaking, mark the commit `!`.
- **Empty success body (204)** - match the generated `NoContent` variant and
  return `true`, as `delete_workflow` does.
- **New required field on a response model** - add it to every default fixture
  in `src/testing.rs` that feeds that model and to any fixture inlined in a
  test; assert it in one happy-path test so a future drop fails loudly.
- **Nullable field decoding as non-optional** - a new nullable pattern the
  normalizer does not know. Teach `xtask` (with a unit test) and regenerate.
- **New endpoint** - one commit carrying all four parts: the method in
  `src/client/`, the `Route` plus its fixture in `testing::default_response`,
  the `EXPECTED_ROUTE_NAMES` entry with happy-path and error tests, and the row
  in the `CLAUDE.md` named-route table.
- **Changed behaviour on an existing argument** - keep the old call compiling
  with `#[deprecated]` and send the new shape on the wire. Never change what an
  argument means in silence.

## 5. Commit and ship

Conventional Commits, one concern per commit, `!` whenever a downstream crate
would stop compiling (see `CLAUDE.md`). Retitle the human PR from the bot's.

Leave the version in `Cargo.toml` alone - there is nothing to bump by hand. On
merge to main, `publish.yml` sees the new `info.version` in `src/openapi.yaml`
and releases the next minor (`0.N+1.0`). That release is gated on `src/`,
`Cargo.toml` or `Cargo.lock` changing since the last tag, so a merge carrying
only docs, tests or skill edits ships nothing.

Open the human PR, then close the bot PR as superseded by it.

## 6. Gate

`just check` must be fully green: `cargo fmt --check`, `clippy -D warnings`,
all tests, docs with `-D warnings`, the `native-tls` build. Refresh the test
counts in `CLAUDE.md` and confirm `EXPECTED_ROUTE_NAMES`, `Route::ALL` and the
named-route table still agree.

## Known gaps

Mention these where they bite; do not fix them as a side effect of an update.

- Bot PRs get no CI run at all (they are opened with `GITHUB_TOKEN`).
- oas3-gen is a single-maintainer project. Bumping it is its own `chore:` PR:
  change the `justfile` pin and the `oas3-gen-support` pin together,
  regenerate, and review the whole generated diff.
