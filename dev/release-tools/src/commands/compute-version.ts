import type { ArgumentsCamelCase, Argv } from "yargs";
import type { GlobalArgs, ReleaseType } from "../types";
import { getSdkConfig } from "../lib/sdk-config";
import {
  computeVersion as computeVersionFn,
  getTimestamp,
  validateTimestamp,
} from "../lib/version";
import { getShortSha } from "../lib/git";

export const command = "compute-version";
export const describe = "Compute the full version string for a release type";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK name (e.g. ios)",
    })
    .option("releaseType", {
      type: "string",
      demandOption: true,
      choices: ["dev", "rc", "final", "nightly"] as const,
      describe: "Release type",
    })
    .option("rcNumber", {
      type: "number",
      describe: "RC number (required for rc releases)",
    })
    .option("sourceRef", {
      type: "string",
      describe:
        "Git ref the release is cut from (e.g. refs/heads/main); main-cut builds get the ordered pre.* format",
    })
    .option("timestamp", {
      type: "string",
      describe:
        "Shared UTC YYYYMMDDHHMMSS stamp for this release run (defaults to now)",
    });
}

export function handler(
  argv: ArgumentsCamelCase<
    GlobalArgs & {
      sdk: string;
      releaseType: ReleaseType;
      rcNumber?: number;
      sourceRef?: string;
      timestamp?: string;
    }
  >,
) {
  if (argv.releaseType === "rc" && argv.rcNumber == null) {
    throw new Error("--rc-number is required when --release-type is 'rc'");
  }
  // Validated up front, before the release type decides whether the stamp is
  // even used: a malformed one always means broken plumbing.
  const runTimestamp = validateTimestamp(argv.timestamp);

  const config = getSdkConfig(argv.sdk);
  const baseVersion = config.manifest.readVersion(argv.repoRoot);
  const needsSha = argv.releaseType === "dev" || argv.releaseType === "nightly";
  const shortSha = needsSha ? getShortSha(argv.repoRoot) : undefined;
  const fromMain =
    (argv.sourceRef ?? "").replace(/^refs\/heads\//, "") === "main";
  // Nightlies are schedule-cut from main by construction, so they always take
  // the ordered pre.* timeline; devs only when the ref says they came off main.
  const needsTimestamp =
    argv.releaseType === "nightly" || (argv.releaseType === "dev" && fromMain);
  // The run-wide stamp when the Release workflow threaded one in; otherwise a
  // fresh one, for standalone/manual invocations.
  const timestamp = needsTimestamp
    ? (runTimestamp ?? getTimestamp())
    : undefined;
  const version = computeVersionFn(baseVersion, argv.releaseType, {
    rcNumber: argv.rcNumber,
    shortSha,
    timestamp,
    fromMain,
  });
  console.log(version);
}
