# Settings

Every setting is an environment variable, and wokesy reads them only when it starts. [Deploy](deploy.md#the-secret) says how they reach wokesy in Kubernetes, and [Development](development.md) how they reach it on your machine.

Only five are required. Without the routing key, `page` is off. Without the wake schedules, `wake me` is off.

## How values are read

- **Durations** are written like `30s`, `5m`, `1h`, or `7d`.
- **Lists** are separated by commas. Spaces around the commas are allowed, and an empty piece is skipped.
- **Digits.** A value made only of digits is read as a number, and `true` and `false` are read as booleans. So a channel name or a topic name cannot be all digits, unless you write it in double quotes, as in `WOKESY_ANNOUNCEMENTS__CHANNEL='"2026"'`.
- **Unknown variables.** Wokesy does not start with a variable it does not know under `WOKESY_`, so a misspelled name is caught at once.

## The variables

The variables marked as secret are declared in `secretspec.toml`.

| Variable | Default | Secret | What it sets |
| --- | --- | --- | --- |
| `WOKESY_ZULIP__SITE` | required | yes | URL of the Zulip organization |
| `WOKESY_ZULIP__EMAIL` | required | yes | Email address of the bot |
| `WOKESY_ZULIP__API_KEY` | required | yes | API key of the bot |
| `WOKESY_ZULIP__INBOX__INITIAL_BACKOFF` | `1s` | | First wait after reading messages from Zulip fails. Each wait after that is longer |
| `WOKESY_ZULIP__INBOX__MAX_BACKOFF` | `1m` | | Longest such wait |
| `WOKESY_PAGERDUTY__TOKEN` | required | yes | PagerDuty REST API key, not read-only |
| `WOKESY_PAGERDUTY__BASE_URL` | `https://api.pagerduty.com/` | | PagerDuty REST API. EU accounts use `https://api.eu.pagerduty.com/` |
| `WOKESY_PAGERDUTY__EVENTS__ROUTING_KEY` | not set: paging is off | yes | Integration key of the Events API V2 integration that `page` alerts |
| `WOKESY_PAGERDUTY__EVENTS__URL` | `https://events.pagerduty.com/` | | PagerDuty Events API. EU accounts use `https://events.eu.pagerduty.com/`. Set it only together with the routing key: wokesy does not start with the URL alone |
| `WOKESY_ANNOUNCEMENTS__CHANNEL` | required | yes | Channel for the on-call announcements and the pages |
| `WOKESY_ANNOUNCEMENTS__TOPIC` | `on call` | yes | Topic in that channel |
| `WOKESY_ON_CALL__REFRESH_INTERVAL` | `1m` | | How often wokesy reads who is on call |
| `WOKESY_ON_CALL__ESCALATION_POLICIES` | empty: every policy that a service uses | yes | Ids of the PagerDuty escalation policies that wokesy shows and announces, separated by commas |
| `WOKESY_PEOPLE__LINKS` | empty | yes | [Links](zulip.md#links) from PagerDuty email addresses to Zulip user IDs |
| `WOKESY_PEOPLE__REFRESH_INTERVAL` | `5m` | | How often wokesy reads the PagerDuty users and the Zulip members |
| `WOKESY_WAKE__SCHEDULES` | empty: `wake me` is off | yes | Ids of the PagerDuty schedules that `wake me` puts the sender on, legacy or shift-based, separated by commas. See [the escalation policy](pagerduty.md#the-escalation-policy) for which schedule to name |
| `WOKESY_WAKE__CHANNELS` | empty: only the announcements channel | yes | Channels where wokesy takes commands, separated by commas |
| `WOKESY_WAKE__LONGEST` | `7d` | | Longest time `wake me` may set |
| `WOKESY_WAKE__PAGE_COOLDOWN` | `5m` | | How long wokesy refuses to page again after a page. `0s` turns the wait off |
| `WOKESY_STATUS__CHANNEL` | not set: status is only logged | yes | Channel for the status posts |
| `WOKESY_HEALTH__INTERVAL` | `30s` | | How often wokesy checks that PagerDuty and Zulip work |
| `WOKESY_HEALTH__TIMEOUT` | `5s` | | How long one check may take |
| `WOKESY_HEALTH__RETRIES` | `2` | | How many times a failed check is tried again before the service counts as unavailable |
| `WOKESY_HEALTH__INITIAL_BACKOFF` | `1s` | | Wait before the first retry. Each wait after that is twice as long |
| `WOKESY_HTTP__HOST` | `127.0.0.1` | | Address that `/livez` and `/readyz` listen on. The container image sets `0.0.0.0` |
| `WOKESY_HTTP__PORT` | `8080` | | Port that they listen on |
| `WOKESY_TELEMETRY__LOG_FORMAT` | `json` | | `json` or `pretty` |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | not set: nothing is exported | yes | URL of an OpenTelemetry (OTLP) collector |
| `OTEL_EXPORTER_OTLP_HEADERS` | not set | yes | Headers for the collector, such as its credentials |
| `RUST_LOG` | `info,wokesy=debug` | | Which log lines are written, in the syntax of `tracing`'s `EnvFilter` |
