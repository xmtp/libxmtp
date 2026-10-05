// Run one bench request in this Node process. Usage: node node.mjs host.json
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

import { publicApi } from "./sdk.mjs";
import { seed, measure } from "./workload.mjs";

// SDK diagnostics go to stderr. Stdout holds one JSON response.
console.log = (...values) => console.error(...values);
const config = JSON.parse(await readFile(process.argv[2], "utf8"));
let input = "";
for await (const part of process.stdin) input += part;
const request = JSON.parse(input);
const root = request.state_directory;
const load = async (name) =>
  JSON.parse(await readFile(join(root, name), "utf8"));
const save = (name, value) =>
  writeFile(join(root, name), JSON.stringify(value));

const sdk = await import(pathToFileURL(config.sdk_entry).href);
const accounts = await import(pathToFileURL(config.accounts_entry).href);
// The Node package root also exports the standard codecs.
const api = publicApi(sdk, sdk, "node", config.backend_url, accounts);
const fixture = await load("fixture.json");
const streamState = `stream-${request.sample}.json`;
let response = { ready: true };
if (request.phase === "setup") {
  const paths = { sender: join(root, "page-sender.db") };
  await save("page.json", await seed(api, fixture, paths, false));
} else if (request.phase === "reset") {
  if (request.workload === "stream") {
    const paths = {
      sender: join(root, `stream-${request.sample}-sender.db`),
      receiver: join(root, `stream-${request.sample}-receiver.db`),
    };
    await save(streamState, await seed(api, fixture, paths, true));
  }
} else {
  const state = await load(
    request.workload === "stream" ? streamState : "page.json",
  );
  response = await measure(
    api,
    state,
    request.workload,
    join(root, `${request.workload}-${request.sample}.db`),
  );
  response.peak_memory_bytes = process.resourceUsage().maxRSS * 1024;
}
process.stdout.write(JSON.stringify(response) + "\n");
