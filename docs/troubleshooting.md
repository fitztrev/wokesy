# Troubleshooting

Find what you see in the first column. The commands use the namespace of [Deploy](deploy.md).

```sh
kubectl -n wokesy logs deployment/wokesy
kubectl -n wokesy logs deployment/wokesy --previous
```

The second command shows the log of the pod that ran before the last restart.

## It does not start, or it stays silent

| You see | Likely cause | Where to look |
| --- | --- | --- |
| The pod restarts in a loop | A required setting is missing, or a variable is misspelled. Or Kubernetes adds `WOKESY_` variables for a Service, because [`enableServiceLinks`](deploy.md#the-deployment) is not `false`. | The first lines of the previous pod's log. They name the setting. |
| The pod never becomes Ready | PagerDuty refuses the key, because it is wrong, revoked, or for the other [region](pagerduty.md#what-to-set-up). Or PagerDuty cannot be reached. | The log and the [status posts](announcements.md#status-posts). They say whether wokesy will keep trying or a person must fix it. |
| No `On call now` post | The name of the announcements channel is wrong, or the bot cannot post there. | The log: `on-call announcement delivery failed`, with Zulip's reason. |
| No reply to a mention | The message did not [start with the mention](commands.md#how-to-give-a-command), the bot is not a Generic bot, or the bot cannot see the channel. | The log: `zulip request failed`, with the wait before the next try. |
| Every post and every reply comes twice | Two copies of wokesy run. | [Run exactly one replica](deploy.md#run-exactly-one-replica). |

## A reply you did not expect

| You see | Likely cause | Where to look |
| --- | --- | --- |
| `I only take commands in the open, …` | The channel is not in `WOKESY_WAKE__CHANNELS`. [Names must match exactly](zulip.md#the-channels), including upper and lower case. | The reply lists the channels that wokesy accepts. |
| `I'm still reading PagerDuty.` for more than a minute | Reading PagerDuty, or the members of Zulip, keeps failing. | Warnings in the log: `keeping the previous roster` or `keeping the previous directory`. |
| `I don't know which PagerDuty user you are. …` | No link, no shared email address, and no identical full name. | Add a [link](zulip.md#links) for the sender. |
| `Waking is not set up here. …` | `WOKESY_WAKE__SCHEDULES` is missing. | The Secret. |
| `Paging is not set up here. …` | `WOKESY_PAGERDUTY__EVENTS__ROUTING_KEY` is missing. | The Secret. |
| `I couldn't page: …` | PagerDuty refused the alert, or cannot be reached. The routing key may be wrong, or `WOKESY_PAGERDUTY__EVENTS__URL` may be missing for an EU account. | The reply has PagerDuty's reason. |
| `… it is not at level 1 of any escalation policy in use …` | The schedule in `WOKESY_WAKE__SCHEDULES` is not the first level of a policy that wokesy shows. | Compare the ids of the schedule and of the policy with PagerDuty. |
| `I couldn't confirm the shift: …` or `I couldn't confirm the override: …` | PagerDuty's answer was lost, or wokesy could not read it. | The schedule in PagerDuty. If the shift or the override is there, remove it by hand when it is no longer needed. |
| `I have no override or shift of yours on record. …`, but PagerDuty shows one | Wokesy has restarted since it made it, or a person made it. | [Wokesy only ends what it made itself](commands.md#wokesy-only-ends-what-it-made-itself). Remove it in PagerDuty. |

## The wrong people

| You see | Likely cause | Where to look |
| --- | --- | --- |
| After `wake me`, an alert reaches everyone at once | `WOKESY_WAKE__SCHEDULES` names a schedule that already has people on it. On a shift-based schedule, `wake me` adds the sender alongside them. | [The escalation policy](pagerduty.md#the-escalation-policy). |
| The roster shows level 1 as `nobody`, with a warning sign | Nothing is wrong. With the recommended escalation policy, level 1 is empty until someone says `wake me`. | [The escalation policy](pagerduty.md#the-escalation-policy). |
| The roster says `No escalation policy is used by a service, …` | An id in `WOKESY_ON_CALL__ESCALATION_POLICIES` is wrong, or no PagerDuty service uses that policy. | The Secret, and the policy in PagerDuty. |
| Someone is shown by a plain name, not by their Zulip name | Wokesy did not match them. | [The log of matches](zulip.md#the-log-of-matches). |
| Someone is shown as the wrong person | They were matched by name, and another Zulip member has that name now. | The log of matches. Add a [link](zulip.md#links) to settle it. |
