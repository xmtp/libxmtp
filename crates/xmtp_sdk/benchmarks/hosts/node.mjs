import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve, join } from "node:path";
import { pathToFileURL } from "node:url";

import { publicApi } from "./sdk.mjs";
import { seed, measure } from "./workload.mjs";

// SDK diagnostics go to stderr. Stdout contains one machine-readable result.
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
const sdk = await import(pathToFileURL(resolve(config.sdk_entry)).href);
const pure = config.pure_entry
  ? await import(pathToFileURL(resolve(config.pure_entry)).href)
  : sdk;
const accounts = await import(
  pathToFileURL(resolve(config.accounts_entry)).href
);
const api = publicApi(sdk, pure, "node", config.backend_url, accounts);
let response;
if (request.phase === "setup") {
  await mkdir(root, { recursive: true });
  await save("request.json", request);
  const fixture = JSON.parse(await readFile(request.fixture, "utf8"));
  await save("fixture.json", fixture);
  const state = await seed(
    api,
    fixture,
    { sender: join(root, "page-sender.db"), receiver: join(root, "unused.db") },
    false,
  );
  await save("page.json", state);
  response = { ready: true };
} else {
  const fixture = await load("fixture.json");
  if (request.phase === "reset") {
    if (request.workload === "stream") {
      const state = await seed(
        api,
        fixture,
        {
          sender: join(root, `stream-${request.sample}-sender.db`),
          receiver: join(root, `stream-${request.sample}-receiver.db`),
        },
        true,
      );
      await save(`stream-${request.sample}.json`, state);
    }
    response = { ready: true };
  } else {
    const state =
      request.workload === "stream"
        ? await load(`stream-${request.sample}.json`)
        : await load("page.json");
    response = await measure(
      api,
      fixture,
      state,
      request.workload,
      join(root, `${request.workload}-${request.sample}.db`),
    );
    response.peak_memory_bytes = process.resourceUsage().maxRSS * 1024;
    response.source = {
      fixture_sha256: request.fixture_sha256,
      package_sha256: request.package_sha256,
    };
  }
}
process.stdout.write(JSON.stringify(response) + "\n");
