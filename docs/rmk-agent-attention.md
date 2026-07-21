# RMK Agent Attention

`rmk-attentiond` indicates blocking Codex and Claude Code threads using three
RMK lighting-overlay cells. It is deliberately local-only: Codex is observed
through the Codex Desktop app-server on loopback, Claude Code posts lifecycle
hooks to a loopback HTTP endpoint, and keyboard updates use `glove80-control`.
No OpenAI or Anthropic API token is needed.

## Signals

The daemon allocates pending threads to F1 through F3. Approvals have priority,
then older requests. More than three requests turn F3 into an overflow signal.

| State | Signal |
| --- | --- |
| Codex | cyan |
| Claude Code | orange |
| Approval | fast blink |
| User input | slow breathe |
| More than three requests | magenta blink on F3 |

The Glove80's stable RMK LED IDs are F1 = 34, F2 = 28, and F3 = 22. The daemon
only sets or unsets these cells; it never clears or replaces the full overlay.
Alerts have a 90-second TTL and are refreshed every 30 seconds, so an unclean
daemon exit cannot leave a permanent alert behind. Removing an alert reveals
the keyboard's lower-priority compiled or runtime layer lighting.

## Run locally

Validate and run from this repository:

```sh
just attention-check
just attention-run --dry-run
just attention-run
```

The daemon discovers a running Codex Desktop `codex app-server
--remote-control` process through `/proc` and connects to its loopback
WebSocket. Use `--codex-url` to override discovery. Run `rmk-attentiond --help`
for keyboard transport, listener, TTL, and executable-path options.

## Configure Claude Code

Merge [`examples/claude-hooks.json`](../examples/claude-hooks.json) into the
`hooks` object in `~/.claude/settings.json`. The example sends only event JSON
to `http://127.0.0.1:37893/claude-hook`; failures are non-blocking and use a
two-second timeout.

The events establish and clear state as follows:

- `PermissionRequest` and `permission_prompt` set approval attention.
- `idle_prompt` and `elicitation_dialog` set input attention.
- Prompt submission, tool completion/failure, stop, denial, and session
  lifecycle events clear stale attention for that Claude session.

## Package

The flake's default package contains `rmk-attentiond`:

```sh
nix build
nix run . -- --dry-run
```

The daemon invokes an RMK-aware `glove80-control` separately. For an installed
user service, pass its packaged absolute path with `--glove80-control` rather
than the development wrapper in this repository.
