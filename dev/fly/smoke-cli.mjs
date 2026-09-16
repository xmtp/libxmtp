import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { parseEnv } from "node:util";
import { fileURLToPath } from "node:url";

// Run only against the persistent test backend. Keys never enter argv or output.
const root = fileURLToPath(new URL("../../", import.meta.url));
const cli = join(root, "apps/cli/bin/run.js");
const key = parseEnv(readFileSync(join(root, ".env"), "utf8")).XMTP_BACKEND_DEV_API_KEY;
assert(key, "Set XMTP_BACKEND_DEV_API_KEY in the root .env");
const backend = "https://backend-dev.xmtp.to";
const directory = mkdtempSync(join(tmpdir(), "xmtp-fly-smoke-"));
process.once("exit", () => {
  // Delete only this run's private identities, databases, and SQLite sidecars.
  rmSync(directory, { recursive: true, force: true });
  console.log("Removed temporary CLI identities and databases.");
});
process.once("SIGINT", () => process.exit(130));
process.once("SIGTERM", () => process.exit(143));
const started = new Date().toISOString();
const secrets = [key];
const safe = (text) => secrets.reduce((value, secret) => value.replaceAll(secret, "[REDACTED]"), text);

function run(identity, args, json = true) {
  const result = spawnSync(process.execPath, [cli, ...args, ...(json ? ["--json"] : [])], {
    cwd: directory,
    encoding: "utf8",
    timeout: 60000,
    killSignal: "SIGKILL",
    env: {
      ...process.env,
      ...identity,
      XMTP_API_KEY: key,
      XMTP_BACKEND_URL: backend,
      XMTP_LOG_LEVEL: "off",
      XMTP_DISABLE_DEVICE_SYNC: "true",
      XMTP_APP_VERSION: "fly-cli-smoke",
    },
  });
  if (result.status !== 0) {
    throw new Error(safe(`CLI ${args[0]} ${args[1] ?? ""} failed: ${result.stderr || result.error?.message || "unknown error"}`));
  }
  return json ? JSON.parse(result.stdout) : result.stdout;
}

function identity(name) {
  const envPath = join(directory, name, ".env");
  run({}, ["init", "--backend-url", backend, "--output", envPath], false);
  const env = parseEnv(readFileSync(envPath, "utf8"));
  secrets.push(env.XMTP_WALLET_KEY, env.XMTP_DB_ENCRYPTION_KEY);
  env.XMTP_DB_PATH = join(directory, name, "messages.db3");
  const info = run(env, ["client", "info"]);
  assert.equal(info.properties.isRegistered, true);
  assert.equal(info.options.backendUrl, backend);
  console.log(`${name}: registered ${info.properties.inboxId}`);
  return { env, info };
}

console.log(`Smoke check started: ${started}`);
console.log(`Private CLI state directory: ${directory}`);
const alice = identity("alice");
const bob = identity("bob");
assert.notEqual(alice.info.properties.inboxId, bob.info.properties.inboxId);
const dm = run(alice.env, ["conversations", "create-dm", bob.info.properties.address]);
run(bob.env, ["conversations", "sync"]);
const expected = [];
for (let index = 1; index <= 3; index++) {
  for (const [name, sender] of [["alice", alice], ["bob", bob]]) {
    const text = `Fly CLI smoke ${started}: ${name} message ${index}`;
    const result = run(sender.env, ["conversation", "send-text", dm.id, text]);
    assert.equal(result.success, true);
    expected.push({ id: result.messageId, content: text, senderInboxId: sender.info.properties.inboxId });
  }
}
for (const [name, recipient] of [["alice", alice], ["bob", bob]]) {
  const messages = run(recipient.env, ["conversation", "messages", dm.id, "--sync", "--kind", "application"]);
  const actual = messages.map(({ id, content, senderInboxId }) => ({ id, content, senderInboxId }));
  const byId = (a, b) => a.id.localeCompare(b.id);
  assert.deepEqual(actual.sort(byId), [...expected].sort(byId));
  console.log(`${name}: all ${expected.length} message IDs, bodies, and senders match`);
}
console.log(JSON.stringify({ started, finished: new Date().toISOString(), conversationId: dm.id, messageIds: expected.map(({ id }) => id) }, null, 2));
