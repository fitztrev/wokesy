# Commands

This page has every command: who may use it, the wordings wokesy accepts, what it does, and what it replies. For what wokesy posts without being asked, see [Announcements](announcements.md).

| Command | Also accepted | Who may use it |
| --- | --- | --- |
| [`page example.com is down`](#page) | | Anyone in a channel where wokesy takes commands |
| [`wake me for 4 hours`](#wake-me) | `woke me` | A sender whom wokesy can match to a PagerDuty user |
| [`stop waking me`](#stop-waking-me) | Any message whose first word is `stop` | Anyone. It ends only what wokesy made for the sender |
| [`back to defaults`](#back-to-defaults) | `reset` or `back to` at the start, or the word `default` or `defaults` anywhere | A sender whom wokesy can match to a PagerDuty user |
| [`who is on call`](#who-is-on-call) | `on call`, `oncall`, `status`, or any message that starts with `who` | Anyone |
| `help` | | Anyone |

`help` lists the commands. To anything else, wokesy answers ``I don't know that command. Send `help` to see the commands I know.``

[How people are matched](zulip.md#how-people-are-matched) says which senders wokesy can match to a PagerDuty user.

## How to give a command

Start a channel message with a mention of wokesy, in a channel where it takes commands:

```
@**Wokesy** who is on call
```

Wokesy answers in the same topic.

- The mention must be the first thing in the message. Wokesy ignores a message that mentions it later on, such as `Page @**Wokesy** if it is urgent`, and a message that mentions only a group it belongs to, or everyone.
- Commands are not case-sensitive, and a `.`, `!`, or `?` at the end is ignored.
- Punctuation after a command word is ignored, so `page: db is down` works.
- A command word in backticks or quotes is not a command. Wokesy does not act on `` `page` is how you alert on call ``, because that message talks about the command.
- When a message fits two commands, the one named at the start wins. `page the default route is down` is a page, not `back to defaults`.

## Commands work only in the open

Wokesy takes commands only in the channels listed in `WOKESY_WAKE__CHANNELS`. If that is not set, it takes them only in the channel of the on-call announcements. [Zulip](zulip.md#the-channels) says how to choose the channels.

It never takes a command by direct message, so the team sees every page and every change. To a direct message, or a mention in another channel, it answers with the channels it does use, and it does nothing else:

```
I only take commands in the open, in #sysadmin. Mention me there.
```

It does not answer a direct message that also goes to other people.

## Right after a start

Until wokesy has read who is on call and has matched the people, it answers `I'm still reading PagerDuty. Try again in a minute.` to `wake me`, `stop waking me`, `back to defaults`, and `who is on call`. `page` and `help` never wait for that.

## One command at a time

Wokesy answers one command at a time. It waits at most 10 seconds for PagerDuty to create or delete one override or custom shift, so a slow PagerDuty cannot hold up a `page` for long.

## `page`

`page example.com is down` alerts whoever PagerDuty has on call. The sender does not need a PagerDuty account.

A page cannot be aimed at a person. The text after `page` is only what the alert says, with the sender's name in front, and PagerDuty decides who gets it. The alert follows the escalation rules of the PagerDuty service as they stand at that moment, including any override or custom shift. Every page is its own alert. Wokesy cuts an alert text that is longer than 1024 characters.

Wokesy answers `Paged. PagerDuty is alerting whoever is on call.` and adds who is on call now, when it knows. It never says who exactly was alerted, because it does not know which service the alert goes to.

Every page is also repeated in the on-call topic, with a silent mention of the person who sent it and their text as a quote, unless the page was sent in that topic itself. Every mention in the quoted text is made silent, so a page cannot make wokesy notify a person, a group, or everyone.

`page` works even while wokesy cannot read the rest of PagerDuty.

| Wokesy replies | When |
| --- | --- |
| `What should I tell them? …` | There is no text after `page`. |
| `Paging is not set up here. Use PagerDuty directly.` | `WOKESY_PAGERDUTY__EVENTS__ROUTING_KEY` is not set. |
| `I couldn't page: …. Use PagerDuty directly.` | PagerDuty refused the alert, or did not answer. |
| `… already paged 2m ago. I won't page again for another 3m. …` | Someone paged less than 5 minutes ago. |

### The wait after a page

After a page, wokesy refuses to page again for 5 minutes, whoever asks. It says who paged, how long ago, and how long it will wait, and it sends nothing to PagerDuty. PagerDuty keeps escalating the first alert until someone acknowledges it.

Only a page that PagerDuty accepted starts this wait. To change it, set `WOKESY_WAKE__PAGE_COOLDOWN`. `0s` turns it off. Wokesy forgets the last page when it restarts.

## `wake me`

`wake me for 4 hours` puts the sender on call for that long, from now until the time is up, on each schedule named in `WOKESY_WAKE__SCHEDULES`. It touches no other schedule. `woke me` works the same way.

What that does to alerts depends on the kind of the schedule and on its place in the escalation policy. [PagerDuty](pagerduty.md#the-escalation-policy) has the layout that puts the sender first.

PagerDuty ends it by itself when the time is up, even if wokesy is not running then. `stop waking me` ends it early. The on-call topic then shows the change. A second `wake me` adds a second entry, and `stop waking me` ends both.

### The duration

The duration is a whole number and a unit:

- minutes: `m`, `min`, `mins`, `minute`, `minutes`
- hours: `h`, `hr`, `hrs`, `hour`, `hours`
- days: `d`, `day`, `days`

`a` and `an` mean one. Several amounts are added together, and other words are ignored. All of these work:

```
wake me for 4 hours
wake me 4h
wake me for 90 minutes
wake me for 1h30m
wake me for 1 hour 30 minutes
wake me for an hour
wake me for anything in the next 4 hours
```

The longest time wokesy sets is 7 days. To change the limit, set `WOKESY_WAKE__LONGEST`.

### The reply

The reply has one line for each schedule in `WOKESY_WAKE__SCHEDULES`. Zulip shows it like this:

```
I'll wake you for anything until 20:00.
- Primary rota: you are on it. It is level 1 of Infrastructure.
- Spare rota: you are on it, but it is not at level 1 of any escalation policy in use, so alerts may not reach you first.
- Night rota: I couldn't override it: pagerduty rejected the request (403 Forbidden): Forbidden
PagerDuty hands it back to the schedule by itself when the time is up. Send `stop waking me` to end it sooner.
```

- For each schedule, the reply names the escalation policies that have it at level 1. When level 1 of such a policy also names people directly, the reply adds `Level 1 also pages … directly.`
- A schedule that is in no escalation policy of the roster is shown by its id.
- On a shift-based schedule the line says `you are on it, alongside whoever the schedule has`, because nobody is replaced. On a schedule with nobody on it, that is nobody.
- When everything wokesy made for this reply is a custom shift, the last line is ``PagerDuty ends the shift by itself when the time is up. Send `stop waking me` to end it sooner.``
- When it could put the sender on no schedule at all, the first line is `I couldn't put you on any schedule.`

| Wokesy replies | When |
| --- | --- |
| `Waking is not set up here. Change the schedule in PagerDuty directly.` | `WOKESY_WAKE__SCHEDULES` is empty, so `wake me` is off. |
| `I don't know which PagerDuty user you are. …` | Wokesy cannot match the sender to a PagerDuty user. |
| ``For how long? For example: `wake me for 4 hours`.`` | The message has no duration, or a duration of zero. |
| `That is longer than I'm allowed to set (7days). Ask for less, or change the schedule in PagerDuty.` | The duration is longer than `WOKESY_WAKE__LONGEST`. |
| `I couldn't override it: …` or `I couldn't add a shift to it: …` | PagerDuty refused, and it created nothing. |
| `I couldn't confirm the override: …. It may exist; check PagerDuty.` or the same with `the shift` | PagerDuty's answer was lost, or wokesy could not read it. |

After `I couldn't confirm`, wokesy does not know whether the override or the shift exists. It has no record of it, so `stop waking me` cannot end it. Look at the schedule in PagerDuty, and remove it there if it exists.

## `stop waking me`

`stop waking me` ends early what `wake me` started. Wokesy deletes the overrides and the custom shifts that it made for the sender, and the schedule decides again. Any message whose first word is `stop` does the same.

| Wokesy replies | When |
| --- | --- |
| `Ended your override on Primary rota. The schedule decides again.` or the same with `your shift` | It ended them. An override or a shift that is already gone from PagerDuty counts as ended. |
| `I couldn't end your override on Primary rota: …` or the same with `your shift` | PagerDuty failed. Wokesy keeps its record, so the sender can try again. |
| `I have no override or shift of yours on record. …` | Wokesy made none for the sender, or theirs have ended, or [wokesy has restarted](#wokesy-only-ends-what-it-made-itself). |

## `back to defaults`

`back to defaults` ends every override and every custom shift that wokesy made, for whoever it made them. Every schedule it touched is then back to what PagerDuty would do without wokesy.

A message that starts with `reset` or `back to` does the same. So does any other message that has the word `default` or `defaults` in it, unless it starts as another command. Leave that word out of a message to wokesy unless you mean it.

The reply has one line for each schedule. Zulip shows it like this:

```
Back to the defaults.
- Primary rota: ended the override of Alice Example.
- Night rota: ended the shift of Alice Example, Bob Example.
PagerDuty's own schedules and overrides decide again.
```

| Wokesy replies | When |
| --- | --- |
| `I don't know which PagerDuty user you are. …` | Wokesy cannot match the sender to a PagerDuty user. |
| `Nothing to undo: I have no override or shift of mine on record. …` | Wokesy made none, or they have ended, or [wokesy has restarted](#wokesy-only-ends-what-it-made-itself). |
| `I couldn't end the override of Alice Example: …` on the line of a schedule | PagerDuty failed. Wokesy keeps its record, so the sender can try again. |

## `who is on call`

`who is on call` shows the roster: one line for each level of each escalation policy that wokesy shows, with the time each shift ends. It is the same text as the first [on-call announcement](announcements.md#on-call-announcements). `on call`, `oncall`, `status`, and any message that starts with `who` do the same.

## Wokesy only ends what it made itself

`stop waking me` and `back to defaults` never touch an override or a custom shift that a person entered in PagerDuty, such as a planned swap or holiday cover. PagerDuty has no way to mark one, so wokesy remembers the ones it creates, and it ends only those.

It remembers them in memory. After a restart it has forgotten them, and it can no longer end them early. `stop waking me` then answers that it has no override or shift of the sender on record. They still end on time, because PagerDuty ends them. To end one sooner, remove it in PagerDuty.
