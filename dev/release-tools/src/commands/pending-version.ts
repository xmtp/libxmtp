import type { ArgumentsCamelCase, Argv } from "yargs";
import { readFileSync } from "node:fs";
import type { GlobalArgs } from "../types";
import { parsePendingFromContext } from "../lib/git-cliff";

export const command = "pending-version";
export const describe =
  "Emit the pending libxmtp {version,kind} from `git cliff --bump --context` JSON";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs
    .option("input", {
      type: "string",
      demandOption: true,
      describe: "Path to `git cliff --bump --context` output ('-' for stdin)",
    })
    .option("lastShipped", {
      type: "string",
      demandOption: true,
      describe:
        "Last-shipped libxmtp version (e.g. 1.10.0) to derive the bump kind against",
    })
    .option("require", {
      type: "boolean",
      default: false,
      describe:
        "Exit non-zero when nothing is pending. Default emits {version:null,kind:null} (exit 0), letting the caller skip a no-op nightly cleanly.",
    });
}

export function handler(
  argv: ArgumentsCamelCase<
    GlobalArgs & { input: string; lastShipped: string; require: boolean }
  >,
) {
  const json =
    argv.input === "-"
      ? readFileSync(0, "utf-8")
      : readFileSync(argv.input, "utf-8");
  const pending = parsePendingFromContext(json, argv.lastShipped);
  if (!pending) {
    // "Nothing to release" is a legitimate state (no conventional commits since
    // the last tag). By default emit a null sentinel so a nightly can skip
    // gracefully rather than failing the job; --require forces a hard error.
    if (argv.require) {
      throw new Error("No pending release (git-cliff computed no next version)");
    }
    console.log(JSON.stringify({ version: null, kind: null }));
    return;
  }
  console.log(JSON.stringify(pending));
}
