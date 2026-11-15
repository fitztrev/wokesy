# Development

Read [AGENTS.md](../AGENTS.md) before you change the code. It has the rules for the code, all the project commands, and the release steps.

## Run it on your machine

You need [Nix](https://nixos.org/download/) and [devenv](https://devenv.sh/) 2.4 or later. devenv installs Rust and the project's own commands, such as `run-on-zulip`.

Wokesy run this way talks to the real PagerDuty account and the real Zulip organization that you give it. `wake me` makes real overrides and custom shifts, and `page` alerts real people.

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

[Settings](settings.md) has every variable. In this shell the log is written for people to read, not as JSON.

To stop it, press Ctrl-C. Wokesy stops reading messages, finishes the command it is working on, and exits.

## Tests

To check the formatting and the lint rules, and to run the tests of the code and of the Kubernetes resources:

```sh
devenv test
```

The tests need no PagerDuty account and no Zulip organization. They run wokesy against small local servers that answer as PagerDuty and Zulip do.
