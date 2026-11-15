# Wokesy

Wokesy is a Zulip bot that manages who is on call in PagerDuty. It announces in a Zulip topic who is on call and when that changes, and it lets people page whoever is on call, or put themselves on call for a few hours, by mentioning it in a channel. PagerDuty stays the source of truth: wokesy has no database, and everything it changes lives in PagerDuty.

## What it does

To give wokesy a command, start a channel message with a mention of it:

```
@**Wokesy** wake me for 4 hours
```

| You write | Wokesy |
| --- | --- |
| `page example.com is down` | Alerts whoever PagerDuty has on call. Anyone in the channel can do this. |
| `wake me for 4 hours` | Puts you on call for that long. `woke me` works too. |
| `stop waking me` | Ends that early. |
| `back to defaults` | Ends everything wokesy added, for everyone. |
| `who is on call` | Shows who is on call now, at each level of the escalation policy. |

Without being asked, it posts in one topic who is on call when it starts, and after that every change.

## How it fits into PagerDuty

Wokesy needs an escalation policy that is laid out like this:

1. **Level 1: a schedule with nobody on it.** `wake me` adds you to this schedule, and wokesy changes no other.
2. **Level 2: the regular schedule.**
3. **Level 3: backup.**

While the level 1 schedule is empty, PagerDuty skips it at once, and alerts go to the regular schedule as they do without wokesy. After `wake me`, an alert goes to you alone. It reaches the others only if you do not acknowledge it within the escalation timeout of level 1.

Do not point `wake me` at the regular schedule. On the kind of schedule that PagerDuty creates today, you would be on call together with whoever is scheduled, not before them, and an alert would reach all of you at once. [PagerDuty](docs/pagerduty.md#the-escalation-policy) has the details.

## Know this first

- **Commands work only in the open.** Wokesy takes a command only from a message that starts with a mention of it, in a channel that is approved for commands. It never takes one by direct message.
- **Nobody is pinged in Zulip.** Wokesy writes every name as a silent mention. PagerDuty is what wakes people.
- **It only ends what it added itself.** `stop waking me` and `back to defaults` leave alone whatever a person entered in PagerDuty.
- **It keeps everything in memory.** After a restart, wokesy no longer knows what it added, and it cannot end it early. PagerDuty still ends it on time.
- **It never edits a schedule's rotations or an escalation policy.** It adds and removes its own entries on the schedules you name, and it triggers the alert of a `page`.
- **Run exactly one replica.** Two copies would both announce and both answer.

## Documentation

- [Commands](docs/commands.md): every command, who may use it, the wordings it accepts, and what it replies.
- [Announcements](docs/announcements.md): what wokesy posts without being asked, and when.
- [PagerDuty](docs/pagerduty.md): what to set up in PagerDuty and why, and everything wokesy reads and writes there.
- [Zulip](docs/zulip.md): the bot, the channels, and how people are matched between PagerDuty and Zulip.
- [Settings](docs/settings.md): every environment variable and its default.
- [Deploy](docs/deploy.md): running it in Kubernetes, and a checklist for the first run.
- [Troubleshooting](docs/troubleshooting.md): what you see, the likely cause, and where to look.
- [Development](docs/development.md): running it on your machine, and the tests.

## Run it

Wokesy is one container, `ghcr.io/lakinwecker/wokesy`, with a tag for each release. It reads every setting from environment variables and stores nothing, so it needs no volume and no database. It needs a PagerDuty API key and a Zulip bot. [Deploy](docs/deploy.md) has the Kubernetes resources and the steps.

## License

Wokesy is free software under the GNU Affero General Public License, version 3. See [LICENSE](LICENSE).
