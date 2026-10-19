import { randomBytes } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { pathToFileURL } from "node:url";

import { backgroundControl } from "./peer-background.mjs";
import { runtimeControl, runtimeResponseSettlement } from "./peer-runtime.mjs";

const require = createRequire(
  new URL("../../../node/package.json", import.meta.url),
);
const { privateKeyToAccount, generatePrivateKey } = require("viem/accounts");
const { toBytes } = require("viem");
const sdkDirectory = process.env.XMTP_NODE_SDK_DIR;
if (!sdkDirectory)
  throw new Error("XMTP_NODE_SDK_DIR must select the staged sibling Node SDK");
const manifest = JSON.parse(
  await readFile(join(sdkDirectory, "package.json"), "utf8"),
);
const { Client, Group, metadataFieldRef } = await import(
  pathToFileURL(resolve(sdkDirectory, manifest.main ?? "index.js"))
);
const backendUrl = process.env.XMTP_BACKEND_URL;
if (!backendUrl)
  throw new Error("XMTP_BACKEND_URL must select this worktree's backend");
const resultFile = process.env.XMTP_RN_RESULT;
if (!resultFile)
  throw new Error("XMTP_RN_RESULT must select the assertion report");
const token = randomBytes(24).toString("hex");
const runId = randomBytes(12).toString("hex");
const account = privateKeyToAccount(generatePrivateKey());
const peerAccount = privateKeyToAccount(generatePrivateKey());
const identity = {
  kind: "ethereum",
  identifier: account.address.toLowerCase(),
};
const directory = await mkdtemp(join(tmpdir(), "xmtp-rn-peer-"));
const peer = await Client.create(
  {
    identity: async () => ({
      kind: "ethereum",
      identifier: peerAccount.address.toLowerCase(),
    }),
    kind: async () => ({ kind: "eoa" }),
    sign: async ({ text }) => ({
      kind: "ecdsa",
      value: toBytes(await peerAccount.signMessage({ message: text })),
    }),
  },
  {
    backend: { url: backendUrl },
    deviceSync: false,
    attachments: { allowPrivateNetwork: true },
    storage: { location: { directory } },
  },
);
const outboundText = `RN to Node ${runId}`;
const inboundText = `Node to RN ${runId}`;
const runtime = runtimeControl({ identity, backendUrl }, async (text) => ({
  bytes: Array.from(toBytes(await account.signMessage({ message: text }))),
}));
const background = backgroundControl(
  { identity, backendUrl, peerInboxId: peer.inboxId },
  peer,
  process.env.XMTP_RN_BACKGROUND_PHASE,
);
const requestedMode = process.env.XMTP_RN_REQUESTED_MODE;
const expectedHarness = process.env.XMTP_RN_EXPECTED_HARNESS;
const requiredPhases = JSON.parse(
  process.env.XMTP_RN_REQUIRED_PHASES_JSON ?? "null",
);
if (
  !["foundation", "conformance", "background"].includes(requestedMode) ||
  !/^[a-f0-9]{64}$/.test(expectedHarness ?? "") ||
  !Array.isArray(requiredPhases) ||
  !requiredPhases.length
)
  throw new Error(
    "runner must admit exact mode, harness identity and required phases",
  );
let runtimeResumed = false;
let backgroundForegroundRequested = false;
let groupVerified = false;
let attachmentVerified = false;
let setupUsed = false;
let replySent = false;
let resultWritten = false;
const server = createServer(async (request, response) => {
  response.setHeader("Content-Type", "application/json");
  try {
    if (
      request.method !== "POST" ||
      request.headers.authorization !== `Bearer ${token}`
    ) {
      response.writeHead(403).end("{}");
      return;
    }
    let raw = "";
    for await (const chunk of request) {
      raw += String(chunk);
      if (raw.length > 65536) throw new Error("control request too large");
    }
    const input = JSON.parse(raw);
    if (request.url.startsWith("/background/")) {
      const output = await background(request.url, input);
      if (request.url === "/background/foreground")
        backgroundForegroundRequested = true;
      response.end(JSON.stringify(output));
      return;
    }
    if (request.url.startsWith("/runtime/")) {
      const output = await runtime(
        request.url,
        input,
        request.url === "/runtime/hold-sign"
          ? runtimeResponseSettlement(response)
          : undefined,
      );
      if (request.url === "/runtime/resume") runtimeResumed = true;
      response.end(JSON.stringify(output));
      return;
    }
    let output;
    switch (request.url) {
      case "/setup":
        if (setupUsed)
          throw new Error("one foundation journey per native launch");
        setupUsed = true;
        output = {
          identity,
          backendUrl,
          peerInboxId: peer.inboxId,
          outboundText,
          inboundText,
        };
        break;
      case "/sign":
        output = {
          bytes: Array.from(
            toBytes(await account.signMessage({ message: input.text })),
          ),
        };
        break;
      case "/receive-and-reply": {
        if (replySent) throw new Error("duplicate reply request");
        await peer.conversations.sync();
        const dm = await peer.conversations.getById(input.conversationId);
        if (!dm || dm.kind !== "dm")
          throw new Error("Node sibling did not receive the RN DM");
        await dm.sync();
        const message = await peer.conversations.getMessageById(
          input.messageId,
        );
        if (
          message?.senderInboxId !== input.senderInboxId ||
          message?.content.kind !== "text" ||
          message.content.value !== outboundText
        ) {
          throw new Error(
            "Node sibling received different RN message id, sender or content",
          );
        }
        output = { messageId: await dm.sendText(inboundText) };
        replySent = true;
        break;
      }
      case "/verify-group": {
        await peer.conversations.sync();
        const group = await peer.conversations.getById(input.conversationId);
        if (!(group instanceof Group))
          throw new Error("Node sibling did not receive native group");
        await group.sync();
        const state = await group.state();
        if (
          state.name !== "RN metadata" ||
          state.permissions.policySet.addMember !== "admin" ||
          !(await group.isAdmin(peer.inboxId))
        )
          throw new Error(
            "Node sibling group metadata, permission or admin mismatch",
          );
        const members = (await group.members()).map((member) => member.inboxId);
        if (
          !members.includes(peer.inboxId) ||
          !members.includes(input.senderInboxId)
        )
          throw new Error("Node sibling group membership mismatch");
        const field = await group.metadataValue(metadataFieldRef("groupName"));
        if (
          field?.kind !== "scalar" ||
          field.value.kind !== "string" ||
          field.value.value !== "RN metadata"
        )
          throw new Error("Node sibling metadata field mismatch");
        const text = await peer.conversations.getMessageById(input.textId);
        if (
          text?.content.kind !== "text" ||
          text.content.value !== "RN feature history"
        )
          throw new Error("Node sibling standard content mismatch");
        const custom = await peer.conversations.getMessageById(input.customId);
        if (
          custom?.encoded?.content.length !== 3 ||
          [0, 128, 255].some(
            (value, index) => custom.encoded.content[index] !== value,
          )
        )
          throw new Error("Node sibling custom sliced bytes mismatch");
        groupVerified = true;
        output = { verified: true };
        break;
      }
      case "/verify-attachment": {
        await peer.conversations.syncAll(undefined);
        const message = await peer.conversations.getMessageById(
          input.messageId,
        );
        if (message?.content.kind !== "remoteAttachment")
          throw new Error("Node sibling attachment message missing");
        const file = await peer.attachments.download(message.content.value);
        const metadata = await peer.archives.metadataFromFile(
          file.path,
          new Uint8Array(input.archiveKey),
        );
        if (
          file.filename !== "history.xmtp" ||
          metadata.elements.length === 0 ||
          (await readFile(file.path)).length <= 32
        )
          throw new Error("Node sibling downloaded invalid archive attachment");
        await peer.attachments.deleteLocal(message.content.value);
        if (
          (await peer.attachments.listLocal()).some(
            (entry) => entry.path === file.path,
          )
        )
          throw new Error("Node sibling attachment deletion failed");
        attachmentVerified = true;
        output = { verified: true };
        break;
      }
      case "/result":
        if (resultWritten) throw new Error("duplicate native result");
        if (input.status === "passed") {
          if (
            input.mode !== requestedMode ||
            input.harnessSha256 !== expectedHarness ||
            JSON.stringify(input.phases) !== JSON.stringify(requiredPhases) ||
            !Array.isArray(input.assertions) ||
            !input.assertions.length
          )
            throw new Error(
              "native result omitted admitted mode/harness/completed phases",
            );
          if (requestedMode !== "background" && (!setupUsed || !replySent))
            throw new Error(
              "native foundation omitted independently verified DM exchange",
            );
          if (
            requestedMode === "conformance" &&
            (!groupVerified || !attachmentVerified || !runtimeResumed)
          )
            throw new Error(
              "native conformance omitted independently observed feature/reload phases",
            );
          if (requestedMode === "conformance") runtime.assertObservation();
          if (requestedMode === "background" && !backgroundForegroundRequested)
            throw new Error(
              "native background omitted independently observed OS foreground request",
            );
        }
        await writeFile(
          resultFile,
          JSON.stringify({ ...input, runId }, null, 2) + "\n",
          { flag: "wx" },
        );
        resultWritten = true;
        output = { accepted: true };
        break;
      default:
        response.writeHead(404).end("{}");
        return;
    }
    response.end(JSON.stringify(output));
  } catch (error) {
    console.error(
      error instanceof Error ? error.message : "peer request failed",
    );
    response
      .writeHead(500)
      .end(JSON.stringify({ error: "peer assertion failed" }));
  }
});
server.listen(
  Number(process.env.XMTP_RN_CONTROL_PORT ?? 0),
  "127.0.0.1",
  () => {
    console.log(JSON.stringify({ port: server.address().port, token }));
  },
);
let stopping = false;
async function stop() {
  if (stopping) return;
  stopping = true;
  server.close();
  await peer.end();
  await rm(directory, { recursive: true, force: true });
}
process.once("SIGTERM", () => void stop());
process.once("SIGINT", () => void stop());
