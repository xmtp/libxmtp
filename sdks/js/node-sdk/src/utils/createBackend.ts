import { BackendBuilder, type Backend } from "@xmtp/node-bindings";
import type { NetworkOptions } from "@/types";

export const createBackend = async (
  options: NetworkOptions,
): Promise<Backend> => {
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- Validate options from JavaScript callers.
  if (!options?.backendUrl?.trim()) {
    throw new Error("backendUrl is required");
  }
  const builder = new BackendBuilder(options.backendUrl);
  if (options.env !== undefined) builder.setEnv(options.env);
  if (options.appVersion !== undefined)
    builder.setAppVersion(options.appVersion);
  return builder.build();
};
