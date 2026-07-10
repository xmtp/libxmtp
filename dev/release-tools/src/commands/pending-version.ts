import type { ArgumentsCamelCase, Argv } from "yargs";
import type { BumpType, GlobalArgs } from "../types";
import { parsePendingFromContext } from "../lib/git-cliff";
import { capBumpKind } from "../lib/sdk-version";
import { readInput } from "../lib/io";

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
    })
    .option("maxBump", {
      type: "string",
      choices: ["major", "minor", "patch"] as const,
      describe:
        "Cap the computed bump at this kind (e.g. 'minor' so nightlies never auto-major). Recomputes the version from --last-shipped when clamping.",
    });
}

export function handler(
  argv: ArgumentsCamelCase<
    GlobalArgs & {
      input: string;
      lastShipped: string;
      require: boolean;
      maxBump?: BumpType;
    }
  >,
) {
  const raw = parsePendingFromContext(readInput(argv.input), argv.lastShipped);
  if (!raw) {
    // "Nothing to release" is a legitimate state (no conventional commits since
    // the last tag). By default emit a null sentinel so a nightly can skip
    // gracefully rather than failing the job; --require forces a hard error.
    if (argv.require) {
      throw new Error(
        "No pending release (git-cliff computed no next version)",
      );
    }
    console.log(JSON.stringify({ version: null, kind: null }));
    return;
  }
  const pending = argv.maxBump
    ? capBumpKind(raw, argv.lastShipped, argv.maxBump)
    : raw;
  console.log(JSON.stringify(pending));
}
