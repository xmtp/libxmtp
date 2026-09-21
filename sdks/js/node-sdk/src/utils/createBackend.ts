import {
  AuthCallback,
  BackendBuilder,
  type Backend,
} from "@xmtp/node-bindings";
import type { NetworkOptions } from "@/types";
import { readCredential } from "./auth";

export const createBackend = async (
  options: NetworkOptions,
): Promise<Backend> => {
  // oxlint-disable-next-line typescript/no-unnecessary-condition -- Validate options from JavaScript callers.
  if (!options?.backendUrl?.trim()) {
    throw new Error("backendUrl is required");
  }
  const builder = new BackendBuilder(options.backendUrl);
  if (options.env !== undefined) builder.setEnv(options.env);
  if (options.appVersion !== undefined)
    builder.setAppVersion(options.appVersion);
  if (options.authCallback) {
    const callback = options.authCallback;
    builder.authCallback(new AuthCallback(() => readCredential(callback)));
  }
  return builder.build();
};
