import init, { BackendBuilder, type Backend } from "@xmtp/wasm-bindings";
import type { NetworkOptions } from "@/types/options";

export const createBackend = async (
  options: NetworkOptions,
): Promise<Backend> => {
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- Validate options from JavaScript callers.
  if (!options?.backendUrl?.trim()) {
    throw new Error("backendUrl is required");
  }
  await init();
  let builder = new BackendBuilder(options.backendUrl);
  if (options.env !== undefined) builder = builder.setEnv(options.env);
  if (options.appVersion !== undefined)
    builder = builder.setAppVersion(options.appVersion);
  return builder.build();
};
