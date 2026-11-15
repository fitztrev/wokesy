# Announcements

This page has what wokesy posts without being asked: who is on call, and its own status. For the replies to commands, see [Commands](commands.md).

Wherever wokesy names a person, it uses a silent mention, so [nobody is pinged](zulip.md#nobody-is-pinged).

## On-call announcements

Wokesy reads who is on call from PagerDuty every minute, and again at once after a command that changes it. It posts in the on-call topic, which [Zulip](zulip.md#the-channels) says how to choose.

When it starts, it posts the whole roster once: one line for each level of each escalation policy that a service uses, with the time each shift ends. Zulip shows it like this:

```
On call now
- Infrastructure, level 1: nobody
- Infrastructure, level 2: Alice until 20:00
- Infrastructure, level 3: Bob until 09:00, Carol until 09:00
```

After that, it posts only when the set of people on call at some level changes, and only the levels that changed:

```
On call changed
- Infrastructure, level 1: Alice until 20:00 (was nobody)
```

- A shift that only gets a new end time is not a change.
- A level with nobody on call says `nobody`, with a warning sign. With the [recommended escalation policy](pagerduty.md#the-escalation-policy), level 1 says that whenever nobody has asked to be woken.
- Times are shown to each reader in their own time zone.
- Only an escalation policy that at least one PagerDuty service uses appears. If `WOKESY_ON_CALL__ESCALATION_POLICIES` is set, only the policies named there appear. If none is left, the roster says `No escalation policy is used by a service, so nobody is on call.`
- After a restart, wokesy posts the whole roster again, because it cannot know what changed while it was down.

While wokesy cannot read PagerDuty, it keeps the roster it has and posts nothing. If Zulip does not take an announcement, wokesy tries again after 30 seconds.

To read PagerDuty more or less often, set `WOKESY_ON_CALL__REFRESH_INTERVAL`.

## Pages

Every [`page`](commands.md#page) is repeated in the on-call topic.

## Status posts

If `WOKESY_STATUS__CHANNEL` is set, wokesy posts in that channel:

- when it starts, with its version and a link to the changelog;
- when PagerDuty works for the first time, when wokesy cannot use PagerDuty, and when PagerDuty works again;
- when it shuts down, with the reason and how long it ran.

A post about PagerDuty says whether wokesy will keep trying or whether a person must fix it, such as a revoked key. Wokesy posts when that changes, not at every check.

Each run of wokesy posts in a topic of its own, named after the version, an id of the run, and the date. The topic cannot be set. A quiet channel for bots suits these posts best.

Without that channel, these messages are only logged.
