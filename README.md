# Wokesy

Wokesy is a Zulip bot for a team that uses PagerDuty for on-call. It posts in a Zulip topic who is on call, and it posts again whenever that changes. People mention wokesy in a channel to page whoever is on call, or to put themselves on call for a few hours. Wokesy has no database: it reads what it needs from PagerDuty and Zulip, and PagerDuty keeps every change that wokesy makes.

## What it does

To give wokesy a command, start a channel message with a mention of it:

```
@**Wokesy** wake me for 4 hours
```

| You write | What wokesy does |
| --- | --- |
| `page example.com is down` | Wokesy alerts whoever PagerDuty has on call. Anyone in the channel can do this. |
| `wake me for 4 hours` | Wokesy puts you on call for that long. You can also write `woke me`. |
| `stop waking me` | Wokesy takes you off call before your time is up. |
| `back to defaults` | Wokesy takes off call everyone whom it put on call. |
| `who is on call` | Wokesy shows who is on call now at each level of the escalation policy. |

Wokesy also posts who is on call when it starts, and it posts every change after that. All of these posts go to one topic.

## How it fits into PagerDuty

Wokesy needs an escalation policy with these three levels:

1. **Level 1 is a schedule with nobody on it.** `wake me` adds you to this schedule. Wokesy changes no other schedule.
2. **Level 2 is the regular schedule.** PagerDuty alerts the people on it when level 1 is empty, or when the person at level 1 does not acknowledge.
3. **Level 3 is the backup schedule.** PagerDuty alerts the people on it when nobody at level 2 acknowledges.

PagerDuty skips level 1 while that schedule is empty, and it alerts the regular schedule at once, as it does without wokesy. After you write `wake me`, PagerDuty alerts you alone. It alerts the others only if you do not acknowledge within the escalation timeout that you set for level 1.

Do not let `wake me` use the regular schedule. On every schedule that PagerDuty creates today, it would put you on call together with the people who are already scheduled, not before them, and it would alert all of you at once. [The PagerDuty guide](docs/pagerduty.md#the-escalation-policy) explains why.

## Know this first

- **Wokesy takes commands only in the open.** A command must start with a mention of wokesy, and it must be in a channel that you approved for commands. Wokesy refuses commands in direct messages.
- **Wokesy never pings anyone in Zulip.** It writes every name as a silent mention. Only PagerDuty wakes people.
- **Wokesy ends only what it added itself.** `stop waking me` and `back to defaults` never remove an entry that a person made in PagerDuty.
- **Wokesy keeps everything in memory.** After a restart it no longer knows whom it put on call, so it cannot take them off early. PagerDuty still takes them off when their time is up.
- **Wokesy never edits a rotation or an escalation policy.** It adds and removes its own entries on the schedules that you name, and it sends an alert when someone writes `page`.
- **Run exactly one copy of wokesy.** Two copies would both post every announcement and both answer every command.

## Documentation

- [Commands](docs/commands.md) explains every command: who may use it, which wordings wokesy accepts, and what it replies.
- [Announcements](docs/announcements.md) explains what wokesy posts without being asked, and when it posts.
- [PagerDuty](docs/pagerduty.md) explains what to set up in PagerDuty and why, and it lists everything that wokesy reads and writes there.
- [Zulip](docs/zulip.md) explains how to set up the bot and the channels, and how wokesy matches people between PagerDuty and Zulip.
- [Settings](docs/settings.md) lists every environment variable and its default.
- [Deploy](docs/deploy.md) explains how to run wokesy in Kubernetes, and it has a checklist for the first run.
- [Troubleshooting](docs/troubleshooting.md) lists what you may see, the likely cause, and where to look.
- [Development](docs/development.md) explains how to run wokesy on your machine and how to run the tests.

## Run it

Wokesy runs as one container. The image is `ghcr.io/lakinwecker/wokesy`, and every release has its own tag. Wokesy reads every setting from environment variables and stores nothing, so it needs no volume and no database. You give it a PagerDuty API key and a Zulip bot. [Deploy](docs/deploy.md) has the Kubernetes resources and the steps.

## License

Wokesy is free software under the GNU Affero General Public License, version 3. See [LICENSE](LICENSE).
