# Zulip

This page has what to set up in Zulip, how wokesy writes about people, and how it matches a PagerDuty user to a Zulip user.

## The bot

1. Create a bot of the type **Generic bot**. Other bot types do not work.
2. Set `WOKESY_ZULIP__SITE` to the URL of your Zulip organization, and `WOKESY_ZULIP__EMAIL` and `WOKESY_ZULIP__API_KEY` to the email address and the API key of the bot.

People give a command by mentioning the bot under the name you gave it.

## The channels

- **On-call announcements.** Set `WOKESY_ANNOUNCEMENTS__CHANNEL` to the channel for the [on-call announcements](announcements.md#on-call-announcements). The topic is `on call` unless you set `WOKESY_ANNOUNCEMENTS__TOPIC`.
- **Commands.** Wokesy takes [commands](commands.md#commands-work-only-in-the-open) in the announcements channel. To choose other channels, set `WOKESY_WAKE__CHANNELS` to their names, separated by commas: `sysadmin, on-call`. Spaces around a name are allowed. When it is set, the announcements channel takes commands only if it is in the list.
- **Status.** To get the [status posts](announcements.md#status-posts), set `WOKESY_STATUS__CHANNEL`.

A channel name must match the name in Zulip exactly, including upper and lower case.

Wokesy sees every public channel. If one of these channels is private, subscribe the bot to it.

## Nobody is pinged

Wherever wokesy names a person, it uses a silent mention: Zulip shows the name, and notifies nobody. The people on call already know their schedule, and PagerDuty is what wakes them. A person whom wokesy cannot find in Zulip is shown by their PagerDuty name.

Wokesy also makes every mention silent in the text that it repeats from elsewhere: the names of people, schedules, and escalation policies from PagerDuty, and the text of a page.

## How people are matched

Wokesy has to know which Zulip user a PagerDuty user is, to mention them and to know who sends `wake me`. For each PagerDuty user, it takes the first of these that gives an answer:

1. A link for the user's PagerDuty email address in `WOKESY_PEOPLE__LINKS`.
2. The one Zulip member who has that email address.
3. The one Zulip member who has the same full name, in any case. A member whom a link or an email address already matched to another PagerDuty user is not matched by name.

If none gives an answer, or two Zulip members fit, the person is not matched. Announcements then show their PagerDuty name without a mention, and they cannot use `wake me` or `back to defaults`. They can still use `page` and `who is on call`.

If two PagerDuty users fit the same Zulip member by the same rule, wokesy cannot tell which of them that member is, and the member cannot use `wake me` or `back to defaults` either.

Only active Zulip members who are not bots are matched. Wokesy reads both lists again every 5 minutes. To change that, set `WOKESY_PEOPLE__REFRESH_INTERVAL`.

Wokesy can match by email address only where the bot may see a member's real address. If your Zulip organization hides the addresses from it, people are matched by name, or by a link.

### The log of matches

Wokesy logs every match that is new or changed, with the PagerDuty user id, the Zulip user id, and the rule that matched: `link`, `email`, or `name`. It logs no name and no email address.

In most Zulip organizations, people can change their own name, and a match by name follows it. Check the log if someone is shown as the wrong person, and add a link to settle it.

### Links

`WOKESY_PEOPLE__LINKS` is one line of `email=zulip_user_id` pairs, separated by commas. Spaces are allowed:

```
alice@example.com=12, bob@example.com=34
```

The email address is the person's address in PagerDuty. The number is their Zulip user ID, which Zulip shows in their profile. Wokesy does not start if a pair is malformed.

Leave the links out at first. The log shows who was matched, and you add a link only for each person it missed.
