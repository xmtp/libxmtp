# libxmtp in Docker Sandboxes

One VM with Nix, Claude Code, Codex CLI, Orca, and a public libxmtp clone. No
host folders are mounted and no backend ports are published. Run host commands
from the repo root.

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
dev/sbx/sandbox destroy  # Delete VM disks and keys. Copy out work first.

# Add or replace a provider key.
dev/sbx/sandbox key api.other-provider.example sbx-other-libxmtp
```

Repeat `--api-key HOST PLACEHOLDER` to register several keys at start. Each
hidden prompt names the domain and placeholder. Custom keys stay on the host;
use one placeholder per provider. Pause unlinks Orca to prevent automatic
wakeup; start restores the connection.

The v3 kits in [sbxenv.yaml](sbxenv.yaml) include both CLIs. Kit changes require
a fresh VM; copy out work before recreating it.
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

On the host, enter your OpenAI API key using Docker's native service:

```bash
sbx secret set openai --sandbox libxmtp-claude
```

Then in a VM terminal, log in with the service sentinel and start Codex:

```bash
printf '%s\n' proxy-managed | codex login --with-api-key
cd ~/libxmtp
codex
```

The login stores the sentinel; Docker supplies the API key on requests. The
upstream kit can also expose a native API key through `OPENAI_API_KEY` during
creation/startup, so API-key mode does not guarantee credential isolation.
This uses [OpenAI API-key authentication](https://learn.chatgpt.com/docs/auth#sign-in-with-an-api-key).

For a ChatGPT subscription with host-managed OAuth, run
`sbx secret set openai --oauth` on the host **before creating the VM**, then
start the VM and run `codex`. This OAuth credential is global. A VM created
logged out needs recreation to pick up the kit's OAuth configuration.

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
