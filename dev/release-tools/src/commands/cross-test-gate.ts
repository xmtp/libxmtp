import type { ArgumentsCamelCase, Argv } from "yargs";
import { readFileSync } from "node:fs";
import type { GlobalArgs } from "../types";
import { evaluateGate } from "../lib/cross-test-gate";

export const command = "cross-test-gate";
export const describe =
  "Evaluate whether a nightly may ship given cross-test runs JSON for a SHA";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs
    .option("input", {
      type: "string",
      demandOption: true,
      describe: "Path to gh-api workflow-runs JSON ('-' for stdin)",
    })
    .option("sha", {
      type: "string",
      demandOption: true,
      describe: "Exact release SHA to require a green run for",
    });
}

export function handler(
  argv: ArgumentsCamelCase<GlobalArgs & { input: string; sha: string }>,
) {
  const json =
    argv.input === "-"
      ? readFileSync(0, "utf-8")
      : readFileSync(argv.input, "utf-8");
  const result = evaluateGate(JSON.parse(json), argv.sha);
  // Machine-readable for the workflow + human-readable on stderr.
  console.log(JSON.stringify(result));
  if (!result.pass) {
    console.error(`Gate: SKIP — ${result.reason}`);
    process.exitCode = 0; // skip is not a failure; workflow reads JSON
  } else {
    console.error(`Gate: PASS — ${result.reason}`);
  }
}
