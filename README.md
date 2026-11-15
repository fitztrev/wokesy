# Wokesy

Wokesy is a Zulip bot that manages who is on call in PagerDuty. It announces in a Zulip topic who is on call and when that changes, and it lets people page whoever is on call, or put themselves on call for a few hours, by mentioning it in a channel. PagerDuty stays the source of truth: wokesy has no database, and everything it changes lives in PagerDuty.

## What it does

To give wokesy a command, start a channel message with a mention of it, in a channel where it takes commands:

```
@**Wokesy** who is on call
```

Wokesy answers in the same topic. Commands are not case-sensitive, and a `.`, `!`, or `?` at the end is ignored.

The mention must be the first thing in the message. Wokesy ignores a message that mentions it later on, such as `Page @**Wokesy** if it is urgent`, and a message that mentions only a group it belongs to, or everyone. Punctuation after a command word is ignored, so `page: db is down` works. A command word in backticks or quotes is not a command: wokesy does not act on `` `page` is how you alert on call ``, because that message talks about the command.

**Commands work only in the open.** Wokesy takes commands only in the channels listed in `WOKESY_WAKE__CHANNELS`. If that is not set, it takes them only in the channel of the on-call announcements. It never takes a command by direct message, so the team sees every page and every change. To a direct message, or a mention in another channel, it answers `I only take commands in the open, in #sysadmin. Mention me there.` with the channels it does use, and it does nothing else. Wokesy sees every public channel. To use it in a private channel, subscribe the bot to that channel.

**Nobody is ever pinged.** Wherever wokesy names a person, it uses a silent mention: Zulip shows the name, and notifies nobody. The people on call already know their schedule, and PagerDuty is what wakes them. A person whom wokesy cannot find in Zulip is shown by their PagerDuty name. Wokesy also makes every mention silent in the text that it repeats from elsewhere: the names of people, schedules, and escalation policies from PagerDuty, and the text of a page.

- **On-call announcements.** Wokesy reads who is on call from PagerDuty every minute, and again at once after a command that changes it. When it starts, it posts the whole roster once in the on-call topic: one line for each level of each escalation policy that a service uses, with the time each shift ends. To show only some escalation policies, name them in `WOKESY_ON_CALL__ESCALATION_POLICIES`. After that, it posts only when the set of people on call at some level changes, and only the levels that changed. Zulip shows it like this:

  ```
  On call changed
  - Infrastructure, level 1: Alice until 20:00 (was Bob)
  ```

  A shift that only gets a new end time is not a change. A level with nobody on call says `nobody`, with a warning sign. Times are shown to each reader in their own time zone.

- **`page example.com is down`** alerts whoever PagerDuty has on call. Anyone in a channel where wokesy takes commands can use it, and they do not need a PagerDuty account. A page cannot be aimed at a person: the text after `page` is only what the alert says, with the sender's name in front, and PagerDuty decides who gets it. The alert follows the escalation rules of the PagerDuty service as they stand at that moment, including any override. Every page is its own alert.

  Wokesy answers `Paged. PagerDuty is alerting whoever is on call.` and adds who is on call now, when it knows. It never says who exactly was alerted, because it does not know which service the alert goes to. Every page is also repeated in the on-call topic, with a silent mention of the person who sent it and their text as a quote, unless the page was sent in that topic itself. Every mention in the quoted text is made silent, so a page cannot make wokesy notify a person, a group, or everyone.

  After a page, wokesy refuses to page again for 5 minutes, whoever asks. It says who paged, how long ago, and how long it will wait, and it sends nothing to PagerDuty. PagerDuty keeps escalating the first alert until someone acknowledges it. Only a page that PagerDuty accepted starts this wait. To change it, set `WOKESY_WAKE__PAGE_COOLDOWN`; `0s` turns it off. Wokesy forgets the last page when it restarts.

  `page` works even while wokesy cannot read the rest of PagerDuty. If paging is not set up, or PagerDuty refuses the alert, wokesy says so and tells the sender to use PagerDuty directly.

- **`wake me for 4 hours`** puts the sender on call for that long. Wokesy creates a PagerDuty schedule override for the sender, from now until the time is up, on each schedule named in `WOKESY_WAKE__SCHEDULES`. It overrides no other schedule. If that setting is empty, `wake me` is off, and wokesy answers `Waking is not set up here. Change the schedule in PagerDuty directly.` PagerDuty ends the override by itself and hands the shift back to the schedule, even if wokesy is not running then. The on-call topic then shows the change.

  The reply has one line for each of those schedules. Zulip shows it like this:

  ```
  I'll wake you for anything until 20:00.
  - Primary rota: you are on it. It is level 1 of Lichess.
  - Spare rota: you are on it, but it is not at level 1 of any escalation policy in use, so alerts may not reach you first.
  - Fishnet rota: I couldn't override it: pagerduty rejected the request (403 Forbidden): Forbidden
  PagerDuty hands it back to the schedule by itself when the time is up. Send `stop waking me` to end it sooner.
  ```

  For each schedule, the reply names the escalation policies that have it at level 1. When level 1 of such a policy also names people directly, the reply says that level 1 also pages them. A schedule that is in no escalation policy of the roster is shown by its id. If PagerDuty's answer to an override was lost or could not be read, wokesy does not know whether the override exists. It then says `I couldn't confirm the override` with the error, and `It may exist; check PagerDuty.` Wokesy has no record of such an override, so `stop waking me` cannot end it.

  The duration is a whole number and a unit: minutes (`m`, `min`, `mins`, `minute`, `minutes`), hours (`h`, `hr`, `hrs`, `hour`, `hours`), or days (`d`, `day`, `days`). `a` and `an` mean one. Several amounts are added together, and other words are ignored. All of these work: `wake me for 4 hours`, `wake me 4h`, `wake me for 90 minutes`, `wake me for 1h30m`, `wake me for 1 hour 30 minutes`, `wake me for an hour`, `wake me for anything in the next 4 hours`.

  The longest time wokesy sets is 7 days. For longer, it tells the sender to ask for less or to change the schedule in PagerDuty. To change the limit, set `WOKESY_WAKE__LONGEST`.

- **`stop waking me`** ends that early. Wokesy deletes the overrides that it made for the sender with `wake me`, and the schedule decides again. Any message whose first word is `stop` does the same.

- **`back to defaults`** ends every override that wokesy made, for whoever it made them, so every schedule it touched is back to what PagerDuty would do without wokesy. Only a sender whom wokesy can match to a PagerDuty user may do this. A message that starts with `reset` or `back to`, or that has the word `default` or `defaults` in it, does the same.

- **`who is on call`** shows the same roster as the first announcement. `on call`, `oncall`, `status`, and any message that starts with `who` do the same.

- **`help`** lists the commands.

- **Status posts.** If `WOKESY_STATUS__CHANNEL` is set, wokesy posts in that channel when it starts, when it cannot use PagerDuty and when PagerDuty works again, and when it shuts down. Each run of wokesy posts in a topic of its own. A post about PagerDuty says whether wokesy will keep trying or whether a person must fix it, such as a revoked key. Without that channel, these messages are only logged.

`wake me` and `back to defaults` work only for a sender wokesy can match to a PagerDuty user. See [How people are matched](#how-people-are-matched). Right after it starts, wokesy answers `I'm still reading PagerDuty. Try again in a minute.` to `wake me`, `stop waking me`, `back to defaults`, and `who is on call`, until it has read who is on call and matched the people. `page` and `help` never wait for that.

Wokesy answers one command at a time. It waits at most 10 seconds for PagerDuty to create or delete one override, so a slow PagerDuty cannot hold up a `page` for long.

### Wokesy only ends overrides it made itself

`stop waking me` and `back to defaults` never touch an override that a person entered in PagerDuty, such as a planned swap or holiday cover. PagerDuty has no way to mark an override, so wokesy remembers the ones it creates, and it ends only those.

It remembers them in memory. After a restart it has forgotten them, and it can no longer end them early: `stop waking me` then answers that it has no override of the sender on record. They still end on time, because PagerDuty ends them. To end one sooner, remove it in PagerDuty.

## What it does not do

- **It never edits a schedule or an escalation policy.** The only things wokesy changes in PagerDuty are schedule overrides that it creates itself and later deletes, and the alerts it triggers for `page`.
- **It keeps nothing between restarts.** After a restart, wokesy reads everything from PagerDuty and Zulip again and posts the whole roster again. An override made with `wake me` is not lost, because PagerDuty holds it, but wokesy no longer knows that it made it. The wait after a page starts from nothing too.
- **It ignores escalation policies that no service uses.** Only a policy used by at least one PagerDuty service appears in the roster. If `WOKESY_ON_CALL__ESCALATION_POLICIES` is set, only the policies named there appear.
- **It overrides only the schedules you name.** `wake me` touches the schedules in `WOKESY_WAKE__SCHEDULES` and no others.
- **It does not decide who is paged.** `page` triggers an alert on one PagerDuty service, and PagerDuty does the rest.
- **It does not notify anyone in Zulip.** Every mention it writes is silent. It makes the mentions silent in the text of a page and in every name from PagerDuty before it repeats them.

## Set it up

### PagerDuty

1. Create a REST API key under **Integrations → API Access Keys**. Do not make it read-only: wokesy reads the users, the escalation policies, and who is on call, and it creates and deletes schedule overrides. This key is `WOKESY_PAGERDUTY__TOKEN`.
2. To turn on `page`, open the service that a page should alert, and add an integration of the type **Events API V2**. The integration key that PagerDuty shows for it is the routing key. Set it as `WOKESY_PAGERDUTY__EVENTS__ROUTING_KEY`. Without it, paging is off and `page` says so.
3. To turn on `wake me`, set `WOKESY_WAKE__SCHEDULES` to the ids of the schedules that it may override, separated by commas: `PI7DH85, PFISHNT`. The id of a schedule is the `P…` code at the end of its PagerDuty URL. Most teams name one schedule: the one at level 1 of their escalation policy. Without this setting, `wake me` is off.
4. If the account has escalation policies of other teams, set `WOKESY_ON_CALL__ESCALATION_POLICIES` to the ids of the ones that wokesy should show, separated by commas. The id of an escalation policy is the `P…` code at the end of its PagerDuty URL. Without this setting, wokesy shows every escalation policy that a service uses.
5. If your account is in PagerDuty's EU region, also set `WOKESY_PAGERDUTY__BASE_URL` to `https://api.eu.pagerduty.com/` and `WOKESY_PAGERDUTY__EVENTS__URL` to `https://events.eu.pagerduty.com/`.

### Zulip

1. Create a bot of the type **Generic bot**. Other bot types do not work.
2. Set `WOKESY_ZULIP__SITE` to the URL of your Zulip organization, and `WOKESY_ZULIP__EMAIL` and `WOKESY_ZULIP__API_KEY` to the email address and the API key of the bot.
3. Set `WOKESY_ANNOUNCEMENTS__CHANNEL` to the channel for the on-call announcements. The topic is `on call` unless you set `WOKESY_ANNOUNCEMENTS__TOPIC`. If the channel is private, subscribe the bot to it.
4. Wokesy takes commands in that channel. To choose other channels, set `WOKESY_WAKE__CHANNELS` to their names, separated by commas: `sysadmin, on-call`. Spaces around a name are allowed. When it is set, the announcements channel takes commands only if it is in the list.

### How people are matched

Wokesy has to know which Zulip user a PagerDuty user is, to mention them and to know who sends `wake me`. For each PagerDuty user, it takes the first of these that gives an answer:

1. A link for the user's PagerDuty email address in `WOKESY_PEOPLE__LINKS`.
2. The one Zulip member who has that email address.
3. The one Zulip member who has the same full name, in any case. A member whom a link or an email address already matched to another PagerDuty user is not matched by name.

If none gives an answer, or two Zulip members fit, the person is not matched. Announcements then show their PagerDuty name without a mention, and they cannot use `wake me` or `back to defaults`. They can still use `page` and `who is on call`. Only active Zulip members who are not bots are matched. Wokesy reads both lists again every 5 minutes.

Wokesy logs every match that is new or changed, with the PagerDuty user id, the Zulip user id, and the rule that matched: `link`, `email`, or `name`. In most Zulip organizations, people can change their own name, and a match by name follows it. Check the log if someone is shown as the wrong person, and add a link to settle it.

Wokesy can match by email address only where the bot may see a member's real address. If your Zulip organization hides the addresses from it, people are matched by name, or by a link.

`WOKESY_PEOPLE__LINKS` is one line of `email=zulip_user_id` pairs, separated by commas. Spaces are allowed:

```
alice@example.com=12, bob@example.com=34
```

The email address is the person's address in PagerDuty. The number is their Zulip user ID, which Zulip shows in their profile. Wokesy does not start if a pair is malformed.

## Settings

Every setting is an environment variable. Durations are written like `30s`, `5m`, `1h`, or `7d`. The variables marked as secret are declared in `secretspec.toml`.

Spaces around the commas in a list are allowed, and an empty piece is skipped. A value made only of digits is read as a number, and `true` and `false` are read as booleans. So a channel name or a topic name cannot be all digits, unless you write it in double quotes, as in `WOKESY_ANNOUNCEMENTS__CHANNEL='"2026"'`. Wokesy does not start with a variable it does not know under `WOKESY_`, so a misspelled name is caught at once.

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
| `WOKESY_PEOPLE__LINKS` | empty | yes | Links from PagerDuty email addresses to Zulip user IDs |
| `WOKESY_PEOPLE__REFRESH_INTERVAL` | `5m` | | How often wokesy reads the PagerDuty users and the Zulip members |
| `WOKESY_WAKE__SCHEDULES` | empty: `wake me` is off | yes | Ids of the PagerDuty schedules that `wake me` overrides, separated by commas. The id is the `P…` code in the schedule's PagerDuty URL |
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

To stop it, press Ctrl-C. Wokesy stops reading messages, finishes the command it is working on, and exits.

To check the formatting and the lint rules, and to run the tests of the code and of the Kubernetes resources:

```sh
devenv test
```

The tests need no PagerDuty account and no Zulip organization. They run wokesy against small local servers that answer as PagerDuty and Zulip do.

## Deploy it

Wokesy runs as one container in Kubernetes. The image is `ghcr.io/lichess-org/wokesy:<version>`. It listens on port 8080 and answers `/livez` and `/readyz`. `/livez` answers 200 while wokesy runs. `/readyz` answers 200 only while wokesy can use PagerDuty, and 503 until its first check of PagerDuty is done.

Wokesy keeps running while PagerDuty or Zulip cannot be reached: it logs the failure and tries again. If one of its own tasks stops, the process exits with an error, and Kubernetes starts it again. On `SIGTERM` or `SIGINT` it stops reading messages, finishes the command it is working on for at most 10 seconds, posts that it is shutting down, and exits. It waits at most 5 seconds for Zulip to take that post, at most 2 seconds for the probe connections to close, and at most 10 seconds to send its last telemetry. So a shutdown takes at most 27 seconds, inside the 30 seconds that the Deployment gives it.

Wokesy leaves out a PagerDuty user, on-call entry, escalation policy, or escalation target that it cannot read, such as a user without an email address, and logs a warning with its PagerDuty id. The rest of the list is used as usual.

Wokesy reads every setting from one Kubernetes Secret named `wokesy`. Put the settings that are not secret in it too, if you change them. Create the Secret in the namespace where wokesy will run:

```sh
kubectl create secret generic wokesy \
  --from-literal=WOKESY_ZULIP__SITE=https://example.zulipchat.com \
  --from-literal=WOKESY_ZULIP__EMAIL=wokesy-bot@example.zulipchat.com \
  --from-literal=WOKESY_ZULIP__API_KEY=the-api-key-of-the-bot \
  --from-literal=WOKESY_PAGERDUTY__TOKEN=the-pagerduty-rest-api-key \
  --from-literal=WOKESY_PAGERDUTY__EVENTS__ROUTING_KEY=the-integration-key \
  --from-literal=WOKESY_ANNOUNCEMENTS__CHANNEL=sysadmin \
  --from-literal=WOKESY_WAKE__SCHEDULES=PI7DH85 \
  --from-literal='WOKESY_WAKE__CHANNELS=sysadmin, on-call' \
  --from-literal=WOKESY_STATUS__CHANNEL=bot-status \
  --from-literal='WOKESY_PEOPLE__LINKS=alice@example.com=12, bob@example.com=34'
```

Leave out the lines that you do not need. Only the first four and the announcements channel are required. Without the routing key, `page` is off. Without the wake schedules, `wake me` is off.

The Kubernetes resources are written in KCL, in `deploy/`. Inside `devenv shell`, `render-manifests` prints them as YAML: one Deployment, which runs the version that `deploy/prod/main.k` pins.

```sh
render-manifests | kubectl apply -f -
```

The Deployment turns off Kubernetes' service links (`enableServiceLinks: false`). With them, a Service named `wokesy` in the namespace would add variables such as `WOKESY_SERVICE_HOST` to the pod, and wokesy does not start with a `WOKESY_` variable that it does not know.

Wokesy reads its settings only when it starts. After you change the Secret, restart it:

```sh
kubectl rollout restart deployment/wokesy
```

**Run exactly one replica.** Two copies of wokesy would both post every announcement and both answer every command. The Deployment has one replica and the `Recreate` strategy, so the old pod stops before the new one starts. Do not scale it up.

## Work on it

Read [AGENTS.md](AGENTS.md) before you change the code. It has the rules for the code, all the project commands, and the release steps.

## License

Wokesy is free software under the GNU Affero General Public License, version 3. See [LICENSE](LICENSE).
