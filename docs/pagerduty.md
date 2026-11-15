# PagerDuty

This page has what to set up in PagerDuty and why, and everything wokesy reads and writes there.

## What to set up

1. **A REST API key.** Create it under **Integrations → API Access Keys**. Do not make it read-only: wokesy creates and deletes schedule overrides and custom shifts. This key is `WOKESY_PAGERDUTY__TOKEN`.
2. **A schedule with nobody on it**, named `overrides` or the like. Set `WOKESY_WAKE__SCHEDULES` to its id. This is the only schedule that wokesy changes. Without this setting, `wake me` is off.
3. **That schedule as level 1 of the escalation policy**, above the regular schedule and backup. [The escalation policy](#the-escalation-policy) says why.
4. **The escalation policies to show.** If the account has escalation policies of other teams, set `WOKESY_ON_CALL__ESCALATION_POLICIES` to the ids of the ones that wokesy should show and announce. Without this setting, wokesy shows every escalation policy that a service uses.
5. **An integration for `page`.** Open the service that a page should alert, and add an integration of the type **Events API V2**. The integration key that PagerDuty shows for it is the routing key. Set it as `WOKESY_PAGERDUTY__EVENTS__ROUTING_KEY`. Without it, paging is off and `page` says so.
6. **The region.** If your account is in PagerDuty's EU region, also set `WOKESY_PAGERDUTY__BASE_URL` to `https://api.eu.pagerduty.com/` and `WOKESY_PAGERDUTY__EVENTS__URL` to `https://events.eu.pagerduty.com/`.

The id of a schedule or of an escalation policy is the `P…` code at the end of its PagerDuty URL. To name several, separate them with commas: `PI7DH85, PFISHNT`.

## The escalation policy

| Level | Who is on it | What wokesy does with it |
| --- | --- | --- |
| 1 | A schedule that normally has nobody on it | `wake me` adds the sender to it for the time they ask. |
| 2 | The regular schedule | Nothing. |
| 3 | Backup | Nothing. |

While the level 1 schedule is empty, nothing changes for anyone. PagerDuty's [documentation of escalation policies](https://docs.pagerduty.com/incident-management/schedules/escalation-policies) says:

> If nobody is on call at the first rule … PagerDuty skips that rule and assigns the incident to the next rule that has an on-call responder

After `wake me`, the sender is alone at level 1. An alert goes to that person alone. It goes to everyone else only if that person does not acknowledge it within the escalation timeout of level 1. You choose that timeout in the escalation policy.

This layout has three side effects:

- The `schedule` block of the roster shows level 1 as `nobody` whenever nobody has asked to be woken. That is as it should be: level 2 is on call, and the first line of the roster names the people at level 2.
- The reply to `page` then says `at level 2`, because PagerDuty starts there.
- When the reply to `wake me` has [one line for each schedule](commands.md#the-reply), that line says `you are on it, alongside whoever the schedule has`. On this schedule, that is nobody.

### Do not use the regular schedule for `wake me`

On a shift-based schedule, `wake me` adds a custom shift, and a custom shift replaces nobody. PagerDuty describes it like this:

> the rotation's regular on-call user remains on call, and the custom shift's user is _also_ on call.

So if `WOKESY_WAKE__SCHEDULES` names a schedule that already has people on it, `wake me` does not put the sender first. An alert reaches everyone on that level at once. This happened in the first test of wokesy in production.

## Legacy and shift-based schedules

PagerDuty has two kinds of schedule, and wokesy works with both.

| Kind | `wake me` creates | The sender |
| --- | --- | --- |
| Legacy, built from layers | A schedule override | Replaces whoever was scheduled |
| Shift-based | A custom shift | Is on call alongside whoever is scheduled |

Shift-based schedules are the schedules of PagerDuty's v3 Schedules API. PagerDuty no longer lets new legacy schedules be created, so every new schedule is shift-based.

Wokesy tries the override first. PagerDuty refuses it on a shift-based schedule, and wokesy then creates the custom shift and remembers the kind of that schedule until it restarts. On a shift-based schedule wokesy does not use PagerDuty's override, because that only replaces one member of a rotation, and it cannot fill a time when nobody is scheduled.

In both cases the override or the shift starts at once, and PagerDuty ends it by itself when the time is up, even if wokesy is not running then.

## What wokesy reads

| What | How often | What for |
| --- | --- | --- |
| Whether the key works | Every 30 seconds | [`/readyz`](deploy.md#probes) and the [status posts](announcements.md#status-posts) |
| The users: id, name, and email address | Every 5 minutes | [Matching people](zulip.md#how-people-are-matched) |
| The escalation policies, with their levels and whether a service uses them | Every minute, and at once after a command that changes who is on call | The roster |
| Who is on call | The same | The roster |

Wokesy leaves out a PagerDuty user, on-call entry, escalation policy, or escalation target that it cannot read, such as a user without an email address, and logs a warning with its PagerDuty id. The rest of the list is used as usual.

## What wokesy writes

Wokesy writes only three things to PagerDuty:

1. **Schedule overrides**, which it creates for `wake me` on a legacy schedule and later deletes.
2. **Custom shifts**, which it creates for `wake me` on a shift-based schedule and later deletes.
3. **Alerts** for `page`, through the Events API, on the service that the routing key belongs to. The alert is critical, its source is `Zulip`, and its text is the sender's Zulip name and what they wrote.

Starting wokesy changes nothing in PagerDuty. It writes only when someone gives it a command.

## What wokesy never touches

- **The layers or rotations of a schedule, and escalation policies.** Wokesy never edits them.
- **A schedule that is not named in `WOKESY_WAKE__SCHEDULES`.** It never takes a schedule from the roster.
- **An override or a custom shift that a person entered.** Wokesy deletes [only what it made itself](commands.md#wokesy-only-ends-what-it-made-itself), and it never lists the overrides or shifts of a schedule.
- **Incidents.** Wokesy triggers the alert of a `page` and does nothing more with it. It does not acknowledge or resolve.
- **Who is paged.** PagerDuty decides that from the service and its escalation policy.

## Confirmed in production

On 15 November 2026 wokesy ran against the real PagerDuty and the real Zulip. These worked: reading who is on call, matching people, a custom shift that starts at once, ending it early, and `page`.
