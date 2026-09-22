import init, {
  fetchServerConfiguration as fetchServerConfigurationBinding,
  type ServerConfiguration,
} from "@xmtp/wasm-bindings";

import type { NetworkOptions } from "@/types/options";
import { toServerConfigurationError } from "@/utils/errors";

/**
 * Reads what a backend publishes about itself with no database, no client,
 * and no credential.
 *
 * An app can call this before it builds a client to learn whether the
 * deployment requires auth, which scopes it requires, and which smart contract
 * wallet chains it accepts.
 *
 * @param optionsOrUrl - The backend URL, or network options carrying it
 * @returns The configuration the backend publishes
 */
export const fetchServerConfiguration = async (
  optionsOrUrl: NetworkOptions | string,
): Promise<ServerConfiguration> => {
  const options =
    typeof optionsOrUrl === "string"
      ? { backendUrl: optionsOrUrl }
      : optionsOrUrl;
  // oxlint-disable-next-line typescript/no-unnecessary-condition -- Validate options from JavaScript callers.
  if (!options?.backendUrl?.trim()) {
    throw new Error("backendUrl is required");
  }
  await init();
  try {
    return await fetchServerConfigurationBinding(
      options.backendUrl,
      options.appVersion,
    );
  } catch (error) {
    const typedError = toServerConfigurationError(error);
    if (typedError) throw typedError;
    throw error;
  }
};
