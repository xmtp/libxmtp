import init, { BackendBuilder, type Backend } from "@xmtp/wasm-bindings";

import type { NetworkOptions } from "@/types/options";

import { readCredential } from "./auth";

export const createBackend = async (
  options: NetworkOptions,
): Promise<Backend> => {
  // oxlint-disable-next-line typescript/no-unnecessary-condition -- Validate options from JavaScript callers.
  if (!options?.backendUrl?.trim()) {
    throw new Error("backendUrl is required");
  }
  await init();
  let builder = new BackendBuilder(options.backendUrl);
  if (options.env !== undefined) builder = builder.setEnv(options.env);
  if (options.appVersion !== undefined)
    builder = builder.setAppVersion(options.appVersion);
  if (options.authCallback) {
    const callback = options.authCallback;
    builder.authCallback({
      async on_auth_required() {
        const credential = await readCredential(callback);
        return {
          ...credential,
          expiresAtSeconds: BigInt(credential.expiresAtSeconds),
        };
      },
    });
  }
  return builder.build();
};
