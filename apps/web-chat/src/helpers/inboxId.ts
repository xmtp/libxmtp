import {
  Backend,
  Client,
  XmtpError,
  type BackendOptions,
} from "@xmtp/browser-sdk";

import { isValidHandle } from "@/helpers/handles";
import { queryClient } from "@/helpers/queries";
import { isValidEthereumAddress } from "@/helpers/strings";

export const getInboxIdForAddressQuery = async (
  address: string,
  backend: BackendOptions,
) =>
  queryClient.fetchQuery({
    queryKey: ["getInboxIdForAddress", address, backend.url],
    queryFn: () => getInboxIdForAddress(address, backend),
    staleTime: 0,
    gcTime: Infinity,
  });

export const getInboxIdForAddress = async (
  address: string,
  options: BackendOptions,
): Promise<string | null> => {
  if (!isValidEthereumAddress(address)) return null;
  try {
    const identity = {
      identifier: address.toLowerCase(),
      kind: "ethereum" as const,
    };
    // one client for both calls: with auth on, identity lookups need a
    // credential, and a credential source answers one client
    const backend = await Backend.connect(options);
    const reachable = await Client.canMessage([identity], backend);
    if (!reachable.get(`ethereum:${identity.identifier}`)) return null;
    return await Client.inboxIdFor(identity, backend);
  } catch (error) {
    if (error instanceof XmtpError.IdentityNotFound) return null;
    throw error;
  }
};

export type HandleLookup = {
  inboxId: string;
  // the inbox's current handle, which can differ from the handle looked up
  handle: string;
};

export const getInboxIdForHandleQuery = async (
  handle: string,
  backend: BackendOptions,
) =>
  queryClient.query({
    queryKey: ["getInboxIdForHandle", handle, backend.url],
    queryFn: () => getInboxIdForHandle(handle, backend),
    staleTime: 0,
    gcTime: Infinity,
  });

/**
 * Resolves a handle through the SDK, which checks the answer against the
 * inbox's own identity log. An old handle resolves too; the result
 * carries the inbox's current handle, which the app shows (D32).
 */
export const getInboxIdForHandle = async (
  handle: string,
  options: BackendOptions,
): Promise<HandleLookup | null> => {
  if (!isValidHandle(handle)) return null;
  // one client for both calls, as for an address
  const backend = await Backend.connect(options);
  let inboxId: string;
  try {
    inboxId = await Client.inboxIdFor(
      { identifier: handle, kind: "handle" },
      backend,
    );
  } catch (error) {
    if (error instanceof XmtpError.IdentityNotFound) return null;
    throw error;
  }
  const states = await Client.inboxStates([inboxId], backend);
  return { inboxId, handle: states.at(0)?.handle ?? handle };
};
