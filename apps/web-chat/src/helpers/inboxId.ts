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
    staleTime: Infinity,
    gcTime: Infinity,
  });

export const getInboxIdForAddress = async (
  address: string,
  backendUrl: string,
): Promise<string | null> => {
  if (!isValidEthereumAddress(address)) return null;
  try {
    return await Client.inboxIdFor(
      { identifier: address.toLowerCase(), kind: "ethereum" },
      { url: backendUrl },
    );
  } catch (error) {
    if (error instanceof XmtpError.IdentityNotFound) return null;
    throw error;
  }
};
