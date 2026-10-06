import { createHash, randomUUID } from "node:crypto";
import { appendFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

export async function replyToTrigger(message, inboxId, trigger, response) {
  if (
    message.senderInboxId === inboxId ||
    message.content.kind !== "text" ||
    message.content.value !== trigger
  ) {
    return;
  }
  return await message.reply(response);
}

export async function runBot(trigger, response) {
  const repoRoot = resolve(process.argv[2] ?? process.cwd());
  const backendUrl = process.env.XMTP_BACKEND_URL;
  if (!backendUrl) throw new Error("XMTP_BACKEND_URL is required");
  const backend = new URL(backendUrl);
  if (
    !["http:", "https:"].includes(backend.protocol) ||
    !["localhost", "127.0.0.1", "[::1]"].includes(backend.hostname)
  ) {
    throw new Error("The smoke check requires a local backend URL");
  }

  const sdkRequire = createRequire(
    pathToFileURL(resolve(repoRoot, "sdks/node/package.json")),
  );
  const [{ Client, MessageStream }, { isHex, toBytes }, accounts] =
    await Promise.all([
      import(pathToFileURL(resolve(repoRoot, "sdks/node/dist/entry.js"))),
      import(pathToFileURL(sdkRequire.resolve("viem"))),
      import(pathToFileURL(sdkRequire.resolve("viem/accounts"))),
    ]);
  const { generatePrivateKey, privateKeyToAccount } = accounts;
  const backendKey = createHash("sha256")
    .update(backend.origin)
    .digest("hex")
    .slice(0, 12);
  const directory = resolve(
    repoRoot,
    "target/smoke-check",
    backendKey,
    trigger,
  );
  await mkdir(directory, { recursive: true });
  const keyPath = resolve(directory, "wallet.key");
  try {
    await writeFile(keyPath, generatePrivateKey(), { flag: "wx", mode: 0o600 });
  } catch (error) {
    if (error?.code !== "EEXIST") throw error;
  }
  const key = (await readFile(keyPath, "utf8")).trim();
  if (!isHex(key) || key.length !== 66)
    throw new Error("Invalid local wallet key");
  const account = privateKeyToAccount(key);
  const signer = {
    identity: () =>
      Promise.resolve({
        kind: "ethereum",
        identifier: account.address.toLowerCase(),
      }),
    kind: () => Promise.resolve({ kind: "eoa" }),
    sign: async (request) => ({
      kind: "ecdsa",
      value: toBytes(await account.signMessage({ message: request.text })),
    }),
  };
  const runId = randomUUID();
  const logPath = resolve(directory, "events.jsonl");
  const log = async (event) => {
    const line = JSON.stringify({ runId, bot: trigger, ...event });
    console.log(line);
    await appendFile(logPath, `${line}\n`);
  };
  const client = await Client.create(signer, {
    backend: { url: backendUrl },
    deviceSync: false,
    storage: {
      location: {
        dbPath: resolve(directory, "client.db3"),
        attachmentsDir: resolve(directory, "attachments"),
      },
    },
  });
  console.log(`${trigger} bot inbox ID: ${client.inboxId}`);
  console.log(`Backend: ${backendUrl}`);
  let stream;
  let stopping = false;
  const stop = () => {
    if (stopping) return;
    stopping = true;
    void stream?.end().catch((error) => {
      console.error(error);
      process.exitCode = 1;
    });
  };
  process.once("SIGINT", stop);
  process.once("SIGTERM", stop);
  try {
    // This is the current SDK's stream for all conversations.
    stream = MessageStream.open(client, {});
    await stream.ready();
    await log({
      event: "ready",
      inboxId: client.inboxId,
      backendUrl,
      logPath,
      response,
    });
    for await (const message of stream) {
      await log({
        event: "received",
        id: message.id,
        conversationId: message.conversationId,
        senderInboxId: message.senderInboxId,
        contentKind: message.content.kind,
        text:
          message.content.kind === "text"
            ? message.content.value
            : message.content.kind === "reply" &&
                message.content.body.kind === "text"
              ? message.content.body.value
              : undefined,
      });
      const replyId = await replyToTrigger(
        message,
        client.inboxId,
        trigger,
        response,
      );
      if (replyId !== undefined) {
        await log({
          event: "replied",
          requestId: message.id,
          conversationId: message.conversationId,
          replyId,
          response,
        });
      }
    }
  } finally {
    process.off("SIGINT", stop);
    process.off("SIGTERM", stop);
    try {
      await stream?.end();
    } finally {
      await client.end();
    }
  }
}
