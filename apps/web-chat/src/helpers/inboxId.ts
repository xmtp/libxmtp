import { Client, XmtpError } from "@xmtp/browser-sdk";

import { queryClient } from "@/helpers/queries";
import { isValidEthereumAddress } from "@/helpers/strings";

export const getInboxIdForAddressQuery = async (
  address: string,
  backendUrl: string,
) =>
  queryClient.fetchQuery({
    queryKey: ["getInboxIdForAddress", address, backendUrl],
    queryFn: () => getInboxIdForAddress(address, backendUrl),
    staleTime: 0,
    gcTime: Infinity,
  });

export const getInboxIdForAddress = async (
  address: string,
  backendUrl: string,
): Promise<string | null> => {
  if (!isValidEthereumAddress(address)) return null;
  try {
    const identity = {
      identifier: address.toLowerCase(),
      kind: "ethereum" as const,
    };
    const backend = { url: backendUrl };
    const reachable = await Client.canMessage([identity], backend);
    if (!reachable.get(`ethereum:${identity.identifier}`)) return null;
    return await Client.inboxIdFor(identity, backend);
  } catch (error) {
    if (error instanceof XmtpError.IdentityNotFound) return null;
    throw error;
  }
};
