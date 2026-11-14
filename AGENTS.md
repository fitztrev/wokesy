# AGENTS.md

Wokesy is a Zulip bot that manages who is on call, written in Rust. PagerDuty is the source of truth: wokesy keeps an in-memory copy of who is on call, announces in a Zulip topic when that changes, and takes commands from Zulip.

## Rules

- **No LLM generated comments.** Names and structure are the documentation. Do not write `//` comments or explanatory `///` docs. The only doc comments are doc tests on public API.
- **Human comments sparingly.** Names and structure are the documentation.
- **No stringly typed values.** Model domain values as newtypes or enums that validate on construction. Define string and number newtypes with `nutype`.
- **Screaming, hexagonal architecture.** Top-level modules are named for domains (`on_call`, `wake`, `people`), not for frameworks. Domains define ports as traits (`chat::Messenger`, `people::ChatMembers`). The `zulip` adapter implements them. `pagerduty` holds on-call concepts as PagerDuty defines them plus the API wrapper, and domains use it directly. Dependencies point inward.
- **No database.** Wokesy keeps nothing between restarts. Everything it knows is read from the PagerDuty and Zulip APIs and held in memory, and everything it changes lives in PagerDuty.
- **Plain names, no technical jargon.** Name things for what they do in this system (`Roster`, `Directory`, `Links`), not for the pattern or layer behind them. Do not add words like `Manager`, `Service`, `Handler`, `Helper`, `Impl`, `Wrapper`, or `Factory` for no reason. A technical word belongs in a name only when it is the most direct description of the thing, as with `http` or `json`.
- **Tests live beside the code, not inside it.** Public API gets doc tests. Every other test goes in `foo/tests.rs`, declared in `foo.rs` with `#[cfg(test)] mod tests;`. Never write an inline `mod tests { ... }` block.
- **Modules are `foo.rs` beside `foo/`, never `foo/mod.rs`.** `clippy::mod_module_files` enforces it.
- **Test our code, not theirs.** Tests check decisions wokesy makes. Never test what PagerDuty, Zulip, `serde`, or a library does. Decode external data into `serde` types built from domain newtypes and enums, so a payload of the wrong shape fails to decode or compile instead of needing a test per field.
- **Test adapters against a local HTTP server that speaks the protocol**: `wiremock`, or a small `axum` server where the fake needs state. Never write an in-memory reimplementation of PagerDuty or Zulip. Test domain code that calls PagerDuty with the real `pagerduty::Api` pointed at a fake server. A `Messenger` double that only records what was sent is fine.
- **Build only what is needed.** No speculative methods, endpoints, options, or trait methods.
- **Nothing about a particular account is built in.** Wokesy works with any PagerDuty account and Zulip organization. Never hard-code or default escalation policy names, schedule ids, channel names, or people.
- **All configuration comes from environment variables.** Application settings live under `WOKESY_`, with `__` separating nesting levels (for example `WOKESY_PAGERDUTY__TOKEN`). Every config struct has `#[serde(deny_unknown_fields)]`. OpenTelemetry uses the standard `OTEL_*` variables. Secrets are declared in `secretspec.toml` and injected only into the process that needs them with `secretspec run`. devenv's secretspec integration stays off in `devenv.yaml`, so secrets never enter the shell. Never put secrets in `.env` files or commit them.
- **The links between people, the channel names, and all Zulip settings are secrets.** They reach production through the Kubernetes Secret, never through plain manifest values.
- **Requests to PagerDuty and Zulip go through `app::http_client()`**, so every one carries our User-Agent, `wokesy/<version> (+https://github.com/lichess-org/wokesy)`.
- **Telemetry uses `tracing`.** It is exported through OpenTelemetry (OTLP) when `OTEL_EXPORTER_OTLP_ENDPOINT` is set. Log as structured fields, not interpolated strings.
- **Strict linting.** Clippy runs with `pedantic`, `nursery`, and restriction lints (no `unwrap`, `expect`, `panic`, or indexing outside tests), all with `-D warnings`. Never silence a lint broadly. If a lint must be suppressed, use a narrowly scoped `#[expect(...)]`.
- **Plain tokio tasks and channels, no actor framework.** Each long-running job is an `async fn keep_…` spawned as a task. Tasks talk through `tokio::sync` channels: `watch` for the latest state, `mpsc` for messages, `Notify` for "refresh now".
- **A task does not end because one request failed.** It logs, keeps the previous state, and tries again.

## Layout

| Path | Responsibility |
| --- | --- |
| `src/chat/` | Chat concepts independent of Zulip, and the `Messenger` port |
| `src/people/` | Who is who: `Email`, `Name`, the `ChatMembers` port, `Links`, the `Directory` that matches PagerDuty users to chat users, and `keep_matched`, which refreshes it |
| `src/pagerduty/` | On-call concepts as PagerDuty defines them, and `pagerduty::Api`, a wrapper of the REST API laid out by its paths |
| `src/on_call/` | The `Roster` (who is on call now, per escalation policy and level), `keep_in_sync`, which reads it from PagerDuty, and `keep_announcing`, which posts its changes |
| `src/wake/` | The commands: `page …`, `wake me for …`, `stop waking me`, `who is on call`, `help` |
| `src/zulip/` | Zulip adapter: `Api`, `ZulipMessenger`, `ZulipMembers`, and `keep_reading`, the inbox that long-polls the event queue |
| `src/health/` | `keep_checking`: checks PagerDuty and Zulip on an interval and publishes `Health` |
| `src/status/` | Posts startup, health changes, and shutdown to a per-instance topic |
| `src/app/` | Config, telemetry, `/livez` and `/readyz`, `http_client()`, and wiring |
| `deploy/` | Kubernetes resources in KCL: `wokesy/` is the typed workload, one `Deployment` that reads every setting from the Secret `wokesy`; `prod/main.k` pins the released version |
| `flake.nix` | The release build and the layered container image (`wokesy`), and the `release` and `release-guards` commands |
| `ci/`, `.github/` | Release hooks and the nixpkgs/rust-overlay pin check; the `Build`, `Release`, and `Deploy` workflows |

## Probes

- Wokesy is crash-only. If any long-running task ends, the process exits with an error and Kubernetes restarts it.
- `/livez` always answers 200 while the HTTP server runs. It reads nothing. Never add dependencies to it.
- `/readyz` follows PagerDuty's availability: 200 while the latest `Health` finds PagerDuty available, 503 before the first check and whenever a check of it fails after its retries.

## Workflow

`cargo` exists only inside the devenv shell. Run everything inside `devenv shell`, or as `devenv shell -- <command>` from the repository root.

- `bacon` runs `check`, `clippy`, `test`, or `run`. Switch jobs with `c`, `t`, and `r`.
- `run-on-zulip` runs wokesy with its secrets from secretspec. The first run prompts for missing secrets and saves them through your secretspec provider.
- `devenv test` runs fmt, clippy (`-D warnings`), nextest, and the doc tests.
- `render-manifests` prints the production Kubernetes YAML from KCL. Never hand-write YAML; change the KCL.

Before handing work back, these must be clean: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, and `cargo test --doc`.

## Release and deploy

- Commit subjects on `main` are conventional commits. `feat` asks for a minor bump, other types ask for a patch, and `!` or `BREAKING CHANGE:` asks for a major. Releases read the version from them.
- `release` runs the shared `dull-nix` release command. It sets `[workspace.package]` version, renders `CHANGELOG.md`, tags `v*`, and pushes. Never edit the version or the changelog by hand.
- The `Release` workflow publishes `ghcr.io/lichess-org/wokesy:<version>` (plus `latest` for stable releases) and creates the GitHub release. For stable releases, `Deploy` then pins `deploy/prod/main.k` to the new version.
- `deploy <version>` re-pins production to a version that is already published.
- `ci/nixpkgs-pin.sh` requires `flake.lock` and `devenv.lock` to agree on nixpkgs and rust-overlay, so dev, CI, and the container share a toolchain. Update both together. A git hook runs it when either lock file is committed.
- Wokesy runs as exactly one replica, with the `Recreate` strategy. Two replicas would both announce and both answer.
