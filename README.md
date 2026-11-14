# Wokesy

Wokesy is a Zulip bot that manages who is on call in PagerDuty. It announces in a Zulip topic who is on call and when that changes, and it lets people page whoever is on call, or put themselves first in line for a few hours, by sending it a message. PagerDuty stays the source of truth: wokesy has no database, and everything it changes lives in PagerDuty.

## What it does

To give wokesy a command, send it a direct message, or mention it in a channel message. Wokesy sees every public channel. To use it in a private channel, subscribe the bot to that channel. Commands are not case-sensitive, and a `.`, `!`, or `?` at the end is ignored.

- **On-call announcements.** Wokesy reads who is on call from PagerDuty every minute. When it starts, it posts the whole roster once in the on-call topic: one line for each level of each escalation policy, with the time each shift ends. That first post notifies nobody. After that, it posts only when the set of people on call at some level changes, and only the levels that changed:

  ```
  On call changed
  - Infrastructure, level 1: @Alice until 20:00 (was Bob)
  ```

  Someone who is newly on call at a level gets a real mention, so Zulip notifies them. People who were already on call at that level, and the people listed after `was`, are mentioned silently. A shift that only gets a new end time is not a change. A person whom wokesy cannot find in Zulip is shown by their PagerDuty name. Times are shown to each reader in their own time zone.

- **`page example.com is down`** alerts whoever is on call, through PagerDuty. Anyone who can message the bot can use it, and they do not need a PagerDuty account. The text after `page` becomes the alert, with the sender's name in front. The alert follows the escalation rules of the PagerDuty service as they stand at that moment, including any override. Every page is its own alert. Wokesy answers `Paged. PagerDuty is alerting whoever is on call.` and adds who is on call now, when it knows. It never says who exactly was alerted, because it does not know which service the alert goes to. Every page is also posted in the on-call topic, with a silent mention of the person who sent it. `page` works even while wokesy cannot read the rest of PagerDuty. If paging is not set up, or PagerDuty refuses the alert, wokesy says so and tells the sender to use PagerDuty directly.

- **`wake me for 4 hours`** puts the sender first in line for that long. Wokesy creates a PagerDuty schedule override for the sender, from now until the time is up, on every schedule at level 1 of every escalation policy. Later levels are not touched. PagerDuty ends the override by itself and hands the shift back to the schedule, even if wokesy is not running then. The reply has one line for each escalation policy. It says where the sender is now first in line, and where wokesy could not do it: for example, when level 1 of a policy has no schedule, only people. The on-call topic then shows the change.

  The duration is a whole number and a unit: minutes (`m`, `min`, `mins`, `minute`, `minutes`), hours (`h`, `hr`, `hrs`, `hour`, `hours`), or days (`d`, `day`, `days`). `a` and `an` mean one. Several amounts are added together, and other words are ignored. All of these work: `wake me for 4 hours`, `wake me 4h`, `wake me for 90 minutes`, `wake me for 1h30m`, `wake me for 1 hour 30 minutes`, `wake me for an hour`, `wake me for anything in the next 4 hours`.

  The longest time wokesy sets is 7 days. For longer, it tells the sender to ask for less or to change the schedule in PagerDuty. To change the limit, set `WOKESY_WAKE__LONGEST`.

- **`stop waking me`** ends that early. Wokesy deletes every override of the sender that is in effect on a level 1 schedule. It cannot tell its own overrides from ones made in PagerDuty, so it ends those too.

- **`who is on call`** shows the same roster as the first announcement, and notifies nobody. `who`, `on call`, `oncall`, and `status` do the same.

- **`help`** lists the commands.

- **Status posts.** If `WOKESY_STATUS__CHANNEL` is set, wokesy posts in that channel when it starts, when it cannot use PagerDuty and when PagerDuty works again, and when it shuts down. Each run of wokesy posts in a topic of its own. A post about PagerDuty says whether wokesy will keep trying or whether a person must fix it, such as a revoked key. Without that channel, these messages are only logged.

`wake me` and `stop waking me` work only for a sender wokesy can match to a PagerDuty user. See [How people are matched](#how-people-are-matched).

## What it does not do

- **It never edits a schedule or an escalation policy.** The only things wokesy changes in PagerDuty are schedule overrides, which it creates and deletes, and the alerts it triggers for `page`.
- **It keeps nothing between restarts.** After a restart, wokesy reads everything from PagerDuty and Zulip again and posts the whole roster again. An override made with `wake me` is not lost, because PagerDuty holds it.
- **It ignores escalation policies that no service uses.** Only a policy used by at least one PagerDuty service appears in the roster and gets overrides.
- **It does not decide who is paged.** `page` triggers an alert on one PagerDuty service, and PagerDuty does the rest.

## Set it up

### PagerDuty

1. Create a REST API key under **Integrations → API Access Keys**. Do not make it read-only: wokesy reads the users, the escalation policies, and who is on call, and it creates and deletes schedule overrides. This key is `WOKESY_PAGERDUTY__TOKEN`.
2. To turn on `page`, open the service that a page should alert, and add an integration of the type **Events API V2**. The integration key that PagerDuty shows for it is the routing key. Set it as `WOKESY_PAGERDUTY__EVENTS__ROUTING_KEY`. Without it, paging is off and `page` says so.
3. If your account is in PagerDuty's EU region, also set `WOKESY_PAGERDUTY__BASE_URL` to `https://api.eu.pagerduty.com/` and `WOKESY_PAGERDUTY__EVENTS__URL` to `https://events.eu.pagerduty.com/`.

### Zulip

1. Create a bot of the type **Generic bot**. Other bot types do not work.
2. Set `WOKESY_ZULIP__SITE` to the URL of your Zulip organization, and `WOKESY_ZULIP__EMAIL` and `WOKESY_ZULIP__API_KEY` to the email address and the API key of the bot.
3. Set `WOKESY_ANNOUNCEMENTS__CHANNEL` to the channel for the on-call announcements. The topic is `on call` unless you set `WOKESY_ANNOUNCEMENTS__TOPIC`. If the channel is private, subscribe the bot to it.

### How people are matched

Wokesy has to know which Zulip user a PagerDuty user is, to mention them and to know who sends `wake me`. For each PagerDuty user, it takes the first of these that gives an answer:

1. A link for the user's PagerDuty email address in `WOKESY_PEOPLE__LINKS`.
2. The one Zulip member who has that email address.
3. The one Zulip member who has the same full name, in any case.

If none gives an answer, or two Zulip members fit, the person is not matched. Announcements then show their PagerDuty name without a mention, and they cannot use `wake me` or `stop waking me`. They can still use `page`. Only active Zulip members who are not bots are matched. Wokesy reads both lists again every 5 minutes.

`WOKESY_PEOPLE__LINKS` is one line of `email=zulip_user_id` pairs, separated by commas. Spaces are allowed:

```
alice@example.com=12, bob@example.com=34
```

The email address is the person's address in PagerDuty. The number is their Zulip user ID, which Zulip shows in their profile. Wokesy does not start if a pair is malformed.

## Settings

Every setting is an environment variable. Durations are written like `30s`, `5m`, `1h`, or `7d`. The variables marked as secret are declared in `secretspec.toml`.

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
| `WOKESY_PAGERDUTY__EVENTS__URL` | `https://events.pagerduty.com/` | | PagerDuty Events API. EU accounts use `https://events.eu.pagerduty.com/`. Set it only together with the routing key |
| `WOKESY_ANNOUNCEMENTS__CHANNEL` | required | yes | Channel for the on-call announcements and the pages |
| `WOKESY_ANNOUNCEMENTS__TOPIC` | `on call` | yes | Topic in that channel |
| `WOKESY_ON_CALL__REFRESH_INTERVAL` | `1m` | | How often wokesy reads who is on call |
| `WOKESY_PEOPLE__LINKS` | empty | yes | Links from PagerDuty email addresses to Zulip user IDs |
| `WOKESY_PEOPLE__REFRESH_INTERVAL` | `5m` | | How often wokesy reads the PagerDuty users and the Zulip members |
| `WOKESY_WAKE__LONGEST` | `7d` | | Longest time `wake me` may set |
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

## Run it on your machine

You need [Nix](https://nixos.org/download/) and [devenv](https://devenv.sh/) 2.4 or later. devenv installs Rust and the project's own commands, such as `run-on-zulip`.

Wokesy run this way talks to the real PagerDuty account and the real Zulip organization that you give it. `wake me` makes real overrides, and `page` alerts real people.

Open a shell in the project:

```sh
devenv shell
```

Run wokesy:

```sh
run-on-zulip
```

On the first run, `run-on-zulip` asks you for the secrets that wokesy cannot start without: the Zulip URL, the bot's email address and API key, the PagerDuty key, and the announcements channel. secretspec, a secrets tool that comes with devenv, saves them outside the repository. It does not ask for the optional ones. To set one of those, for example the routing key for `page`:

```sh
secretspec set WOKESY_PAGERDUTY__EVENTS__ROUTING_KEY
```

To check the formatting and the lint rules, and to run the tests:

```sh
devenv test
```

## Deploy it

Wokesy runs as one container in Kubernetes. The image is `ghcr.io/lichess-org/wokesy:<version>`. It listens on port 8080 and answers `/livez` and `/readyz`. `/readyz` answers 200 only while wokesy can use PagerDuty.

Wokesy reads every setting from one Kubernetes Secret named `wokesy`. Put the settings that are not secret in it too, if you change them. Create the Secret in the namespace where wokesy will run:

```sh
kubectl create secret generic wokesy \
  --from-literal=WOKESY_ZULIP__SITE=https://example.zulipchat.com \
  --from-literal=WOKESY_ZULIP__EMAIL=wokesy-bot@example.zulipchat.com \
  --from-literal=WOKESY_ZULIP__API_KEY=the-api-key-of-the-bot \
  --from-literal=WOKESY_PAGERDUTY__TOKEN=the-pagerduty-rest-api-key \
  --from-literal=WOKESY_PAGERDUTY__EVENTS__ROUTING_KEY=the-integration-key \
  --from-literal=WOKESY_ANNOUNCEMENTS__CHANNEL=sysadmin \
  --from-literal=WOKESY_STATUS__CHANNEL=bot-status \
  --from-literal='WOKESY_PEOPLE__LINKS=alice@example.com=12, bob@example.com=34'
```

Leave out the lines that you do not need. Only the first four and the announcements channel are required.

The Kubernetes resources are written in KCL, in `deploy/`. Inside `devenv shell`, `render-manifests` prints them as YAML: one Deployment, which runs the version that `deploy/prod/main.k` pins.

```sh
render-manifests | kubectl apply -f -
```

Wokesy reads its settings only when it starts. After you change the Secret, restart it:

```sh
kubectl rollout restart deployment/wokesy
```

**Run exactly one replica.** Two copies of wokesy would both post every announcement and both answer every command. The Deployment has one replica and the `Recreate` strategy, so the old pod stops before the new one starts. Do not scale it up.

## Work on it

Read [AGENTS.md](AGENTS.md) before you change the code. It has the rules for the code, all the project commands, and the release steps.

## License

Wokesy is free software under the GNU Affero General Public License, version 3. See [LICENSE](LICENSE).
