import path from "node:path";
import type { ArgumentsCamelCase, Argv } from "yargs";
import type { GlobalArgs } from "../types";
import { setDevcontainerImage as setDevcontainerImageFn } from "../lib/devcontainer";

const DEVCONTAINER_JSON_PATH = ".devcontainer/devcontainer.json";

export const command = "set-devcontainer-image";
export const describe =
  "Pin .devcontainer/devcontainer.json to a specific container image reference";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs.option("image", {
    type: "string",
    demandOption: true,
    describe:
      "Fully qualified image reference (e.g. ghcr.io/xmtp/libxmtp-devcontainer@sha256:...)",
  });
}

export function handler(
  argv: ArgumentsCamelCase<GlobalArgs & { image: string }>,
) {
  const jsonPath = path.join(argv.repoRoot, DEVCONTAINER_JSON_PATH);
  setDevcontainerImageFn(jsonPath, argv.image);
  console.log(`Updated ${DEVCONTAINER_JSON_PATH} to image ${argv.image}`);
}
