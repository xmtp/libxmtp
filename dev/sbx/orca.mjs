#!/usr/bin/env node
// Use the installed app's client and declared SSH API so setup follows its UI path.
import { createRequire } from "node:module";
import { join } from "node:path";

const [
  command,
  appPath = "/Applications/Orca.app",
  host = "libxmtp-claude.sbx",
] = process.argv.slice(2);

async function main() {
  if (!["ensure", "lookup", "remove"].includes(command)) {
    throw new Error(
      "Usage: orca.mjs ensure|lookup|remove [ORCA_APP_PATH] [SANDBOX_HOST]",
    );
  }
  if (!/^[a-zA-Z0-9.-]+\.sbx$/.test(host)) {
    throw new Error("SANDBOX_HOST must be a Docker Sandboxes .sbx hostname");
  }
  const require = createRequire(import.meta.url);
  const { RuntimeClient } = require(
    join(
      appPath,
      "Contents/Resources/app.asar.unpacked/out/cli/runtime-client.js",
    ),
  );
  // Explicit nulls prevent inherited Orca variables from routing to another host.
  const client = new RuntimeClient(undefined, 300_000, null, null);
  const status = await client.call("status.get");
  if (!status.result.capabilities?.includes("ssh.target-management.v1")) {
    throw new Error("This Orca app does not support SSH target management");
  }
  const { result } = await client.call("ssh.listEditableTargets");
  const matches = result.targets.filter(
    (target) => target.host === host && target.username === "_default_user_",
  );
  if (matches.length > 1) {
    throw new Error(`Multiple Orca SSH targets match ${host}; keep one target`);
  }
  let target = matches[0];
  if (!target && command === "ensure") {
    const added = await client.call("ssh.addTarget", {
      target: {
        label: "sandbox",
        configHost: host,
        host,
        port: 22,
        username: "_default_user_",
        allowRemoteCliControl: false,
      },
    });
    target = added.result.target;
  }
  if (!target) {
    console.log(JSON.stringify({ targetId: null, environmentId: null }));
    return;
  }
  const response = await client.call(
    command === "ensure" ? "ssh.connect" : "ssh.getState",
    {
      targetId: target.id,
      ...(command === "ensure" ? { automatic: false } : {}),
    },
  );
  const state = response.result.state;
  const managed = state?.managedServer;
  const environmentId =
    managed?.kind === "managed" ? managed.environmentId : null;
  if (
    command === "ensure" &&
    (state?.status !== "connected" || !environmentId)
  ) {
    throw new Error(
      "Orca did not finish setting up its managed sandbox server",
    );
  }
  if (command === "remove") {
    await client.call("ssh.removeTarget", { targetId: target.id });
  }
  console.log(JSON.stringify({ targetId: target.id, environmentId }));
}

main().catch((error) => {
  console.error(`orca sandbox setup: ${error.message}`);
  process.exitCode = 1;
});
