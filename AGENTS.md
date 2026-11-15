# AGENTS.md

Wokesy is a Zulip bot that manages who is on call, written in Rust. PagerDuty is the source of truth: wokesy keeps an in-memory copy of who is on call, announces in a Zulip topic when that changes, and takes commands from channel messages that start with a mention of it: `page …`, `wake me for …`, `stop waking me`, `back to defaults`, `who is on call`, and `help`. [README.md](README.md) says what each does for the people who use it.

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
- **Log ids, never people or what they wrote.** Log a person as a PagerDuty user id or a chat user id. Never log a name, an email address, or the text of a message or of an event. A test that checks what is logged listens with `crate::testing::Logs`.
- **Strict linting.** Clippy runs with `pedantic`, `nursery`, and restriction lints (no `unwrap`, `expect`, `panic`, or indexing outside tests), all with `-D warnings`. Never silence a lint broadly. If a lint must be suppressed, use a narrowly scoped `#[expect(...)]`.
- **Plain tokio tasks and channels, no actor framework.** Each long-running job is an `async fn keep_…` spawned as a task. Tasks talk through `tokio::sync` channels: `watch` for the latest state, `mpsc` for messages, `Notify` for "refresh now".
- **A task does not end because one request failed.** It logs, keeps the previous state, and tries again.

## Decisions that the code must keep

- **Nobody is pinged.** Every person wokesy names is a silent mention, `ChatUserId::silent_mention()` (`@_**|id**`), or their PagerDuty name when the directory has no chat user for them. There is no notifying mention, and none may be added.
- **Text from elsewhere cannot notify.** `chat::silenced` turns every `@*` into `@_*`, which makes a mention of a person, a group, or everyone silent. A name from PagerDuty goes into a message only through `on_call::policy_in_chat`, `on_call::schedule_in_chat`, or `on_call::name_in_chat`, which apply it. `EscalationPolicyName`, `ScheduleName`, and `Name` have no `Display`, so a name cannot be formatted into a message without one of them.
- **A command starts with the bot's own mention.** `zulip::keep_reading` passes on a channel message only when Zulip flags it as a mention and, after leading whitespace, it starts with `@**Name**` or `@**Name|id**` of the bot. A mention later in the message, or of a group or of everyone, is not a command.
- **A quoted command word is talk about the command.** `wake::parsing` ignores punctuation after a command word, but a word that follows a backtick or a quote character is no command word, and a message whose first word is quoted is unrecognized. The reset wordings stay as wide as they are: `reset` or `back to` at the start, or the word `default` or `defaults` anywhere.
- **Commands only in the open.** `wake::Commands::answer` first checks where the message arrived. A direct message, or a mention in a channel that is not in `wake::Config::open_channels`, gets the pointer to the open channels and nothing else, whatever it asks.
- **What someone typed is repeated only silenced and inside a quote.** The page announcement passes the text through `chat::silenced` and then quotes it line by line with `> `. The quote alone is not enough: Zulip does not silence a mention that directly follows `>`. Never put a sender's text in a message in any other way.
- **Wokesy only ends overrides it made itself.** `Commands` records every override it creates, in memory. `stop waking me` and `back to defaults` work only from those records and never list a schedule's overrides. The records are gone after a restart, and the replies say so. PagerDuty still ends those overrides on time.
- **Wokesy overrides only the schedules named in `wake::Config::schedules`.** `wake me` is off when there are none, and says so before anything else. It never takes a schedule to override from the roster.
- **An override whose answer was lost may exist.** After a 4xx, PagerDuty created nothing, and the reply says `I couldn't override it`. After a transport error, a timeout, a 5xx, or a success of an unexpected shape, wokesy has no record and the reply says `I couldn't confirm the override: … It may exist; check PagerDuty.`
- **Wokesy never edits a schedule's layers or an escalation policy.** It creates and deletes schedule overrides, and it triggers alerts through the Events API. Nothing else is written to PagerDuty.
- **Paging does not depend on reading PagerDuty.** `page` and `help` are answered while the roster or the directory is still unread. The page cooldown is one clock for everyone, kept in memory.
- **Commands are answered one at a time.** That keeps the page cooldown and the records of overrides free of races. So that one slow command cannot hold up a page for long, creating or deleting an override times out after 10 seconds, not after the 30 seconds of the other PagerDuty requests.
- **One record that cannot be read does not fail a list.** `pagerduty::Api::users`, `escalation_policies`, and `oncalls` decode each record as an `Entry`, skip one that does not decode, and warn with its PagerDuty id. A page that is not a list of records still fails the read.
- **A member matched by a link or by email is not matched to someone else by name.** `Directory::matching` matches by link and by email first. A match by name with a chat member that those rules matched to anyone is then dropped, and the PagerDuty user stays unmatched. `keep_matched` logs each match that is new or changed, from `Directory::matched_since`.

## Layout

| Path | Responsibility |
| --- | --- |
| `src/chat/` | Chat concepts independent of Zulip. `conversation.rs`: `ChatUserId`, `ChannelName`, `TopicName`, `Conversation`. `incoming.rs` and `outgoing.rs`: `IncomingMessage`, `CommandText`, `OutgoingMessage`. `messenger.rs`: the `Messenger` port and `DeliveryError`. `markup.rs`: the silent mention, `silenced`, and `time_tag` |
| `src/people/` | Who is who. `email.rs` and `name.rs`: `Email`, `Name`. `members.rs`: `ChatMember` and the `ChatMembers` port. `links.rs`: `Links`, read from `email=zulip_user_id` pairs. `directory.rs`: `Directory::matching` matches each PagerDuty user to a chat user by link, then email, then full name, and `matched_since` lists the matches that are new or changed. `refresh.rs`: `keep_matched` reads both sides every `refresh_interval`, publishes the `Directory`, and logs its new matches |
| `src/pagerduty/` | On-call concepts as PagerDuty defines them (`user.rs`, `schedule.rs`, `escalation_policy.rs`, `on_call.rs`), and the two clients. `api.rs` is `pagerduty::Api`, the REST API, and `Entry`, a record of a list that is skipped when it cannot be read, with one file per path under `api/`: `abilities`, `users`, `escalation_policies`, `oncalls`, `schedules` (create and delete an override). `events.rs` is `pagerduty::Events`, the Events API v2 trigger, and `Summary` |
| `src/on_call/` | `roster.rs`: the `Roster`, who is on call now per level of each escalation policy that is in use and chosen. `chosen_policies.rs`: `ChosenPolicies`, the escalation policies named in the configuration, or every policy when none is named. `sync.rs`: `keep_in_sync` reads the roster from PagerDuty on an interval and when asked to refresh. `changes.rs`: what counts as a change, the set of people at a level. `rendering.rs`: `on_call_now`, the "On call changed" text, and `policy_in_chat`, `schedule_in_chat`, and `name_in_chat`, which put a name from PagerDuty into a message. `announcing.rs`: `keep_announcing` posts the roster once and then its changes. `config.rs`: `Config` and `Announcements` |
| `src/wake/` | The commands. `parsing.rs`: a pure function from text to `Command`. `channels.rs`, `schedules.rs`, and `config.rs`: `Channels`, `Schedules`, `Config`, and `open_channels`. `commands.rs`: `Commands::answer`, which refuses anything outside the open channels and then answers `who`, `help`, and the rest. `commands/paging.rs`: `page` and its cooldown. `commands/waking.rs`: `wake me`, on the configured schedules. `commands/own_overrides.rs`: the overrides wokesy made, `stop waking me`, and `back to defaults`. `answering.rs`: `keep_answering` sends each `Answer` back, and a page's announcement to the on-call topic |
| `src/zulip/` | Zulip adapter. `api.rs`: `zulip::Api`. `messenger.rs`: `ZulipMessenger`, the `Messenger`. `members.rs`: `ZulipMembers`, the `ChatMembers`: active people who are not bots. `inbox.rs`: `keep_reading` long-polls the event queue and passes on each direct message and each channel message that starts with the bot's mention |
| `src/health/` | `keep_checking` checks PagerDuty (`abilities`) and Zulip (the bot's own user) on an interval, with retries, and publishes `Health` when it changes. `checks.rs` decides which failures are retried and which need a person |
| `src/status/` | `Status` posts the start, the changes in PagerDuty's availability, and the shutdown to a topic of its own per instance. Without a status channel it only logs them |
| `src/app/` | `app.rs`: `run`, the wiring, the supervision, the shutdown order with its `LongestWaits`, and `http_client()`. `config.rs`: `Config`, read from the environment. `http.rs`: `/livez` and `/readyz`. `telemetry.rs`: logs and OTLP export. `tests.rs`: the end-to-end tests, with the fake PagerDuty and the fake Zulip under `tests/` |
| `src/testing.rs` | `Logs`, for a test that checks what is logged |
| `deploy/` | Kubernetes resources in KCL: `wokesy/` is the typed workload, one `Deployment` that reads every setting from the Secret `wokesy`, and its tests in `workload_test.k`; `prod/main.k` pins the released version |
| `flake.nix` | The release build and the layered container image (`wokesy`), and the `release` and `release-guards` commands |
| `ci/`, `.github/` | Release hooks and the nixpkgs/rust-overlay pin check; the `Build`, `Release`, and `Deploy` workflows |

`testing.rs` in `on_call` and `wake` holds builders that only the tests of that module use.

## Tasks and channels

`app::run` binds the HTTP listener, serves the probes, and spawns six tasks. Together with the HTTP server they are the seven long-running jobs. Only then does it post the start, beside the jobs, so a Zulip that does not answer delays none of them.

| Job | Publishes | Ends by itself when |
| --- | --- | --- |
| `health::keep_checking` | `watch<Option<Health>>`, read by `/readyz` and by `Status::follow` | every receiver is gone |
| `people::keep_matched` | `watch<Option<Directory>>`, read by `keep_announcing` and `Commands` | every receiver is gone |
| `on_call::keep_in_sync` | `watch<Option<Roster>>`, read by `keep_announcing` and `Commands` | never |
| `on_call::keep_announcing` | messages to the announcements topic | the roster channel closes |
| `zulip::keep_reading` | `mpsc<IncomingMessage>`, read by `keep_answering` | the receiver is dropped |
| `wake::keep_answering` | replies, and page announcements | the sender is dropped, after the message in hand |
| HTTP server (`app::http::serve`) | `/livez`, `/readyz` | it is told to shut down |

- A watch channel starts as `None`. `keep_announcing` waits for the first `Directory`, and `Commands` answers `I'm still reading PagerDuty` until both the `Roster` and the `Directory` are there.
- `Commands` and `keep_in_sync` share one `Arc<Notify>`. After `wake me`, `stop waking me`, or `back to defaults` changed overrides, `Commands` notifies it, and `keep_in_sync` reads PagerDuty again at once.
- `Status::started` and `Status::follow` are not tasks. `run` polls them beside the jobs, the start first, so that `Status` is free to post the shutdown afterwards.
- `zulip::keep_reading` waits one backoff step before it registers a new event queue when Zulip rejected the last one before it delivered anything.
- Create every receiver of a channel before spawning the job that sends on it, or the job sees no receiver and ends.

## Probes

- Wokesy is crash-only. If any of the seven jobs ends by itself, `run` shuts the rest down and returns an error that names the job, such as `on-call sync stopped unexpectedly`, or `on-call sync failed: …` with the panic. `main` then exits non-zero, and Kubernetes restarts it.
- A job does not end because PagerDuty or Zulip cannot be reached. Wokesy keeps running, logs the failures, and tries again.
- `/livez` always answers 200 while the HTTP server runs. It reads nothing. Never add dependencies to it.
- `/readyz` follows PagerDuty's availability: 200 while the latest `Health` finds PagerDuty available, 503 before the first check and whenever a check of it fails after its retries. Zulip's availability is checked and logged, and does not change `/readyz`.
- On `SIGINT` or `SIGTERM`, `run` stops the inbox, lets `keep_answering` finish the command in hand for at most 10 seconds, aborts the other tasks, posts the shutdown for at most 5 seconds, shuts the HTTP server down gracefully for at most 2 seconds, and returns `Ok`. `main` then shuts telemetry down, which takes at most 10 seconds. These waits are `LONGEST_WAITS` in `app.rs`. Their sum, 27 seconds, must stay under the pod's `terminationGracePeriodSeconds`, 30. A test in `src/app/tests.rs` holds that.
- The post about the start is given up after 5 seconds too.

## Workflow

`cargo` exists only inside the devenv shell. Run everything inside `devenv shell`, or as `devenv shell -- <command>` from the repository root.

- `bacon` runs `check`, `clippy`, `test`, or `run`. Switch jobs with `c`, `t`, and `r`.
- `run-on-zulip` runs wokesy with its secrets from secretspec. The first run prompts for missing secrets and saves them through your secretspec provider.
- `devenv test` runs fmt, clippy (`-D warnings`), nextest, the doc tests, and `test-manifests`.
- `src/app/tests.rs` starts the whole application against a fake PagerDuty and a fake Zulip on local ports, through `run_until`, which takes the listener and the shutdown request. Add a test there when a change touches how the modules fit together. The tests wait on what the fakes receive, never on a sleep.
- `render-manifests` prints the production Kubernetes YAML from KCL. Never hand-write YAML; change the KCL.
- `test-manifests` runs the KCL tests in `deploy/`, the `*_test.k` files. Add a test there for a property the rendered resources must keep, such as `enableServiceLinks: false`: a Service named `wokesy…` must not add `WOKESY_` variables that stop wokesy from starting.
- A test that waits on a timeout of ours uses tokio's paused clock against a server that accepts the connection and never answers, or a short wait passed in. Do not send a request that should be answered while the clock is paused.

Before handing work back, these must be clean: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, and `cargo test --doc`.

## Release and deploy

- Commit subjects on `main` are conventional commits. `feat` asks for a minor bump, other types ask for a patch, and `!` or `BREAKING CHANGE:` asks for a major. Releases read the version from them.
- `release` runs the shared `dull-nix` release command. It sets `[workspace.package]` version, renders `CHANGELOG.md`, tags `v*`, and pushes. Never edit the version or the changelog by hand.
- The `Release` workflow publishes `ghcr.io/lichess-org/wokesy:<version>` (plus `latest` for stable releases) and creates the GitHub release. For stable releases, `Deploy` then pins `deploy/prod/main.k` to the new version.
- `deploy <version>` re-pins production to a version that is already published.
- `ci/nixpkgs-pin.sh` requires `flake.lock` and `devenv.lock` to agree on nixpkgs and rust-overlay, so dev, CI, and the container share a toolchain. Update both together. A git hook runs it when either lock file is committed.
- Wokesy runs as exactly one replica, with the `Recreate` strategy. Two replicas would both announce and both answer.
