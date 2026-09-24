import type { ArgumentsCamelCase, Argv } from "yargs";
import type { Channel, GlobalArgs, VersionTrack } from "../types";
import { SDK_CONFIGS } from "../lib/sdk-config";

export interface SdkRow {
  sdk: string;
  releaseWorkflow: string;
  versionTrack: VersionTrack;
}

/**
 * Registry rows that ship on a channel and are real fan-out targets
 * (non-empty releaseWorkflow — excludes the libxmtp hub itself).
 */
export function listSdksForChannel(channel: Channel): SdkRow[] {
  return Object.entries(SDK_CONFIGS)
    .filter(
      ([, cfg]) => cfg.releaseWorkflow !== "" && cfg.channels.includes(channel),
    )
    .map(([sdk, cfg]) => ({
      sdk,
      releaseWorkflow: cfg.releaseWorkflow,
      versionTrack: cfg.versionTrack,
    }));
}

export const command = "list-sdks";
export const describe = "List SDK fan-out targets for a channel as JSON";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs.option("channel", {
    type: "string",
    demandOption: true,
    choices: ["nightly", "rc", "final"] as const,
    describe: "Release channel",
  });
}

export function handler(
  argv: ArgumentsCamelCase<GlobalArgs & { channel: Channel }>,
) {
  console.log(JSON.stringify(listSdksForChannel(argv.channel)));
}
