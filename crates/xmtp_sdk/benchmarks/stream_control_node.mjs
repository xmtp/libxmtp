import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { runStreamControls } from "./stream_control.mjs";
const fixture = JSON.parse(await readFile(process.argv[2], "utf8"));
const measured = process.argv[3]
  ? (await import(pathToFileURL(resolve(process.argv[3])).href)).measure
  : undefined;
console.log(
  JSON.stringify(await runStreamControls(fixture, "node", measured), null, 2),
);
