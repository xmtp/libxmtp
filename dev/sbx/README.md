# libxmtp in Docker Sandboxes

One VM with Nix, Claude Code, Codex CLI, Orca, and a public libxmtp clone. No
host folders are mounted and no backend ports are published. Run host commands
from the repo root.

Keep Docker's MCP catalog (`sbx mcp ls`) empty while this VM runs. Its gateway
can reach host-registered servers; start refuses a nonempty catalog.

## Setup once

Install Nix, Orca with its CLI, and Docker Sandboxes:

```bash
brew tap docker/tap
brew trust --cask docker/tap/sbx
brew install --cask docker/tap/sbx
sbx login
sbx policy init balanced
sbx settings set ssh.agentForwardingEnabled false
sbx daemon restart
```

## Host commands

```bash
# First start: enter the Morph key at the hidden prompt.
dev/sbx/sandbox start --api-key api.morphllm.com sbx-morph-libxmtp

dev/sbx/sandbox start    # Create/resume and connect Orca.
dev/sbx/sandbox pause    # Stop; keep files and keys.
dev/sbx/sandbox destroy  # Delete VM disks and keys; retain Claude history.

# Add or replace a provider key.
dev/sbx/sandbox key api.other-provider.example sbx-other-libxmtp
```

Every VM has the Codex CLI. Add `--codex` to the start that creates the VM to
also set up its login; see [Codex session](#codex-session).
Repeat `--api-key HOST PLACEHOLDER` to register several keys at start. Each
hidden prompt names the domain and placeholder. Custom keys stay on the host;
use one placeholder per provider. Pause unlinks Orca to prevent automatic
wakeup; start restores the connection.

The v3 kits in [sbxenv.yaml](sbxenv.yaml) include both CLIs. Kit changes require
a fresh VM; copy out work before recreating it.
Docker 0.50 retains Claude history volumes after destroy and reattaches them
when the same name and kit are recreated. See [Docker's release notes](https://docs.docker.com/ai/sandboxes/release-notes/#0500).
Use `sandbox key` for third-party providers such as Morph. For the kits'
native OpenAI/Anthropic domains, use the service-credential commands below:
`sbx` 0.50 strips custom placeholders on those domains. The published kits
can expose native API keys through guest environment variables; choose
subscription OAuth when API-key isolation is required.

## Claude sessions

### Morph

In an Orca terminal inside the VM, or after `ssh libxmtp-claude.sbx`, run Morph
with its 1M-token context window:

```bash
cd ~/libxmtp
claude --model morph-dsv41flash-abliterated --settings '{
  "env": {
    "ANTHROPIC_BASE_URL": "https://api.morphllm.com",
    "ANTHROPIC_AUTH_TOKEN": "sbx-morph-libxmtp",
    "ANTHROPIC_API_KEY": "",
    "CLAUDE_CODE_MAX_CONTEXT_TOKENS": "1000000",
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "morph-dsv41flash-abliterated",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "morph-dsv41flash-abliterated",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "morph-dsv41flash-abliterated",
    "CLAUDE_CODE_SUBAGENT_MODEL": "morph-dsv41flash-abliterated"
  }
}'
```

For another provider, change the model, Anthropic-compatible URL, context limit,
and placeholder. Each session chooses independently. The aliases keep helper
requests on the same model. `--settings` also accepts a native Claude JSON file;
store placeholders there, never real keys. Use `/status` to check the session.

### Anthropic API key

Store the key at Docker's hidden host prompt, then use its native sentinel
inside the VM. API usage is billed separately from a Claude subscription.

```bash
# Host
sbx secret set anthropic --sandbox libxmtp-claude
# VM
ANTHROPIC_API_KEY=proxy-managed claude --model sonnet
```

### Claude subscription

Leave the host's Anthropic API key unset; API keys take precedence over
subscription login. Inside the VM, start without provider overrides:

```bash
unset ANTHROPIC_API_KEY ANTHROPIC_AUTH_TOKEN ANTHROPIC_BASE_URL
claude
```

Run `/login`, select your Claude subscription account, and open the printed
URL on the host if needed. Paste the login code back into the VM terminal.
Use `/status` to confirm the account. See [Claude authentication](https://code.claude.com/docs/en/authentication).

## Codex session

Codex login is opt-in. With it, Codex uses your ChatGPT subscription through
Docker's host-side OAuth. The tokens stay on the host. The VM holds only the
`oai-oat01-proxy-managed` sentinel, and the proxy replaces it on requests to
`chatgpt.com`.

Create the VM with `--codex`:

```bash
dev/sbx/sandbox start --codex
```

If the host has no global OpenAI secret, `start` first runs
`sbx secret set openai --oauth`. Sign in to ChatGPT at the host prompt. Then,
in a VM terminal:

```bash
cd ~/libxmtp
codex
```

Do not run `codex login` in the VM; the kit pins Codex to the proxy login.

The Codex kit writes `~/.codex/config.toml` and `auth.json` once, at VM
creation, from the host secret that exists then. `--codex` on a later `start`
does not change an existing VM. To add Codex login, copy out work, then
`destroy` and `start --codex`. Without `--codex`, a VM still picks up an
OpenAI secret that already exists on the host. The OAuth secret is global, so
every local sandbox with the Codex kit can use the subscription. Remove it
with `sbx secret rm openai`.

To use an OpenAI API key instead, store it before `start --codex`:

```bash
sbx secret set openai
```

An API key takes precedence over OAuth. The upstream kit can expose a native
API key through `OPENAI_API_KEY` during creation/startup, so API-key mode does
not guarantee credential isolation. This uses
[OpenAI API-key authentication](https://learn.chatgpt.com/docs/auth#sign-in-with-an-api-key).

## Build and test

Inside `~/libxmtp`:

```bash
dev/nix-shell --shell rust 'just backend up'
dev/nix-shell --shell rust 'just backend status'
dev/nix-shell --shell rust 'just test workspace'
```

VM resources and first-boot Nix setup live in [sbxenv.yaml](sbxenv.yaml), using
Docker's experimental [environment files](https://docs.docker.com/ai/sandboxes/configuration/environment-files/).
For Orca failures, see `orca environment recover --help` or
`orca environment update --help` on the host.

Wrapper tests are manual (outside CI discovery):
`dev/nix-shell 'python3.11 -B dev/sbx/test_sandbox.py'`.
