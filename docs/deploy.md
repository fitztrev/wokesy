# Deploy

Wokesy runs as one container in Kubernetes. The image is `ghcr.io/lakinwecker/wokesy:<version>`. It is public, so the cluster needs no pull secret. Use the newest release.

Before you deploy, set up [PagerDuty](pagerduty.md#what-to-set-up) and [Zulip](zulip.md). The examples on this page use a namespace named `wokesy`.

There are two resources: a Secret and a Deployment. There is no Service, no ConfigMap, and no volume claim.

## The Secret

Wokesy reads every setting from one Kubernetes Secret named `wokesy`. Put the settings that are not secret in it too, if you change them. [Settings](settings.md) has the whole list.

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: wokesy
  namespace: wokesy
type: Opaque
stringData:
  WOKESY_ZULIP__SITE: "https://example.zulipchat.com"
  WOKESY_ZULIP__EMAIL: "wokesy-bot@example.zulipchat.com"
  WOKESY_ZULIP__API_KEY: "the-api-key-of-the-bot"
  WOKESY_PAGERDUTY__TOKEN: "the-pagerduty-rest-api-key"
  WOKESY_PAGERDUTY__EVENTS__ROUTING_KEY: "the-integration-key"
  WOKESY_ANNOUNCEMENTS__CHANNEL: "sysadmin"
  WOKESY_WAKE__CHANNELS: "sysadmin, on-call"
  WOKESY_STATUS__CHANNEL: "bot-status"
  WOKESY_ON_CALL__ESCALATION_POLICIES: "PANZZEQ"   # the escalation policy to show
  WOKESY_WAKE__SCHEDULES: "PI7DH85"                # the schedule at level 1 that is normally empty
  # WOKESY_PEOPLE__LINKS: "alice@example.com=12, bob@example.com=34"
```

- Only the first four and the announcements channel are required. Leave out the lines that you do not need.
- Quote every value. A value made only of digits also needs quotes inside the quotes, as in `'"2026"'`.
- Do not commit the Secret in plain text. Encrypt it the way your repository handles secrets, or create it with `kubectl create secret generic wokesy --from-literal=NAME=value …`, with one `--from-literal` for each setting.

## The Deployment

The Kubernetes resources are written in KCL, in `deploy/`. Inside `devenv shell`, `render-manifests` prints them as YAML: one Deployment, which runs the version that `deploy/prod/main.k` pins.

```sh
kubectl create namespace wokesy
kubectl -n wokesy apply -f secret.yaml
render-manifests | kubectl -n wokesy apply -f -
```

If you keep the Deployment in a repository of your own, copy that YAML, and keep these properties:

- **One replica and the `Recreate` strategy.** See [Run exactly one replica](#run-exactly-one-replica).
- **`enableServiceLinks: false`.** With service links, a Service named `wokesy` in the namespace would add variables such as `WOKESY_SERVICE_HOST` to the pod, and wokesy does not start with a `WOKESY_` variable that it does not know.
- **`terminationGracePeriodSeconds: 30`,** or more. See [Shutdown](#shutdown).
- **A writable `/tmp`.** The root file system is read-only, and the container runs as an unprivileged user.

## Run exactly one replica

Two copies of wokesy would both post every announcement and both answer every command. The Deployment has one replica and the `Recreate` strategy, so the old pod stops before the new one starts. Do not scale it up.

## Probes

Wokesy listens on port 8080.

- `/livez` answers 200 while wokesy runs.
- `/readyz` answers 200 only while wokesy can use PagerDuty, and 503 until its first check of PagerDuty is done. Whether Zulip can be reached has no effect on it.

Nothing sends requests to wokesy, so a pod that is not Ready still works as far as it can. Ready only tells you whether wokesy can use PagerDuty.

## Start

Starting wokesy changes nothing in PagerDuty. It reads, and it posts the roster in the on-call topic. The log says `wokesy started`.

If a required setting is missing, or a `WOKESY_` variable is misspelled, wokesy exits at once and names the setting. Kubernetes then restarts it in a loop.

## While it runs

Wokesy keeps running while PagerDuty or Zulip cannot be reached: it logs the failure and tries again. If one of its own tasks stops, the process exits with an error, and Kubernetes starts it again.

The log is JSON, one line for each event. It names people only by their PagerDuty and Zulip ids. It never has a person's name, an email address, a key, or the text of a message.

## Shutdown

On `SIGTERM` or `SIGINT` wokesy stops reading messages, finishes the command it is working on for at most 10 seconds, posts that it is shutting down, and exits. It waits at most 5 seconds for Zulip to take that post, at most 2 seconds for the probe connections to close, and at most 10 seconds to send its last telemetry. So a shutdown takes at most 27 seconds, inside the 30 seconds that the Deployment gives it.

## What a restart forgets

Wokesy keeps nothing between restarts. After a restart, it reads everything from PagerDuty and Zulip again and posts the whole roster again.

- An override or a custom shift made with `wake me` is not lost, because PagerDuty holds it. But wokesy no longer knows that it made it, and [cannot end it early](commands.md#wokesy-only-ends-what-it-made-itself).
- The [wait after a page](commands.md#the-wait-after-a-page) starts from nothing.

## Change a setting

Wokesy reads its settings only when it starts. After you change the Secret, restart it:

```sh
kubectl -n wokesy rollout restart deployment/wokesy
```

## A new version

`release`, which [AGENTS.md](../AGENTS.md#release-and-deploy) describes, publishes the image of the new version and, for a stable release, pins it in `deploy/prod/main.k`. Nothing is applied to the cluster by that. Pull the repository and apply the Deployment again, or change the tag of the image in your own copy.

## First run

This order is safe on a real on-call. Steps 1 to 4 change nothing. From step 5 on you act on the real schedule and the real service, and you put yourself first before you page, so that the test page wakes nobody else. Tell the channel that a bot is about to appear.

First check that it came up:

```sh
kubectl -n wokesy rollout status deployment/wokesy
kubectl -n wokesy logs deployment/wokesy -f
```

The pod turns Ready once the first check of PagerDuty passes. The status channel, if you set one, gets the start post and `PagerDuty works`. If not, see [Troubleshooting](troubleshooting.md).

1. **The on-call topic.** It has `On call now` with the real roster, and the people are shown under their Zulip names. Nobody got a notification. This shows that the PagerDuty key works.
2. **The log.** It has a line for each person matched, with the ids and the rule. You must be among them. Whoever is missing needs a [link](zulip.md#links).
3. **A question.** Send `@**Wokesy** who is on call`. The same roster comes as a reply in that topic. This shows that the bot receives mentions.
4. **Two messages that are no commands.** Send the bot a direct message, `who is on call`. It answers `I only take commands in the open, …`. Then write `ask @**Wokesy** who is on call` in the channel. That gets no reply at all.
5. **Put yourself first.** Send `@**Wokesy** wake me for 30 minutes`. The reply says that you are on the empty schedule and that it is level 1 of your escalation policy. In PagerDuty, that schedule shows a shift for you. `On call changed` shows level 1 going from nobody to you. You are now alone at level 1. If the reply says anything else, [stop here](#if-step-5-goes-wrong).
6. **One test page.** From a topic other than the on-call topic, send `@**Wokesy** page >@**Your Name** wokesy test, please ignore`. The reply says `Paged.` PagerDuty opens an incident on the real service and alerts you, because you are first. Acknowledge and resolve it at once, before it escalates to everyone. The page is repeated in the on-call topic as a quote that shows your name, and that gives you no Zulip notification.
7. **A second page, at once.** Wokesy refuses, names you, and says how long is left of the 5 minutes. Nothing reaches PagerDuty.
8. **Back to normal.** Send `@**Wokesy** stop waking me`. The reply says `Ended your shift on …`, or `Ended your override on …` if the schedule is a legacy one. The shift is cut off in PagerDuty, and `On call changed` shows level 1 back to nobody. Alerts go to the regular schedule again.
9. **Optional: what wokesy did not make.** Add a shift for yourself by hand on the empty schedule in PagerDuty, then send `@**Wokesy** back to defaults`. The reply says `Nothing to undo: …`, and your shift is still there. Remove it by hand.
10. **Optional: a restart.** Send `@**Wokesy** wake me for 10 minutes`, restart the pod as in [Change a setting](#change-a-setting), and send `@**Wokesy** stop waking me`. The old pod posts that it is shutting down and stops within seconds. The new one answers `I have no override or shift of yours on record. …` The shift ends on time, or you remove it in PagerDuty.

### If step 5 goes wrong

Do not page until it is fixed, or the page goes to everyone. In every case, copy the reply and the log lines around it.

- **`I couldn't add a shift to it: …`**, or **`I couldn't override it: …`**. PagerDuty refused, and nothing was created.
- **`I couldn't confirm the shift: …. It may exist; check PagerDuty.`** PagerDuty's answer was lost, or wokesy could not read it. Look at the schedule in PagerDuty. The shift is probably there, and you are first, but wokesy has no record of it. Remove it by hand when you are done.
- **`… it is not at level 1 of any escalation policy in use …`** The schedule in `WOKESY_WAKE__SCHEDULES` is not the first level of the policy. Fix the policy, or the setting.

## Back out

- **Stop wokesy.** `kubectl -n wokesy scale deployment/wokesy --replicas=0`. PagerDuty carries on exactly as it does without wokesy.
- **A shift you do not want.** Delete it on the page of the schedule in PagerDuty. What wokesy made also ends by itself at the time it told you.
- **A test incident.** Resolve it in PagerDuty.
- **Remove it entirely.** `kubectl delete namespace wokesy`, then deactivate the bot in Zulip, and delete the API key and the integration in PagerDuty.
