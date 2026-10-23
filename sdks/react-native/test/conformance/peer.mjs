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
const { Client, sdkVersion } = await import(
  pathToFileURL(resolve(sdkDirectory, manifest.main ?? "index.js"))
);
if (sdkVersion() !== (process.env.XMTP_SDK_RELEASE_VERSION ?? manifest.version))
  throw new Error(
    "Node native SDK version differs from the expected package version",
  );
const backendUrl = process.env.XMTP_BACKEND_URL;
if (!backendUrl)
  throw new Error("XMTP_BACKEND_URL must select this worktree's backend");
// `pr` (the default) runs the RN-only journeys; `full` adds runtime reload and
// the cold background launch.
const journeys = process.env.XMTP_RN_JOURNEYS || "pr";
if (journeys !== "pr" && journeys !== "full")
  throw new Error("XMTP_RN_JOURNEYS must be pr or full");
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
if (requestedMode === "background" && journeys !== "full")
  throw new Error("the cold background launch belongs to the full journey set");
let runtimeResumed = false;
let backgroundForegroundRequested = false;
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
          journeys,
          sdkVersion: sdkVersion(),
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
          if (requestedMode === "conformance" && input.journeys !== journeys)
            throw new Error("native conformance ran a different journey set");
          if (
            requestedMode === "conformance" &&
            runtimeResumed !== (journeys === "full")
          )
            throw new Error(
              "native conformance runtime reload does not match the journey set",
            );
          if (requestedMode === "conformance" && journeys === "full")
            runtime.assertObservation();
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
