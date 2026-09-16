import {
  createBackend,
  getInboxIdForIdentifier,
  IdentifierKind,
} from "@xmtp/browser-sdk";
import { backendLabel } from "@/helpers/backend";
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
  const env = await backendLabel(backendUrl);
  const backend = await createBackend({ backendUrl, env });
  const inboxId = await getInboxIdForIdentifier(backend, {
    identifier: address.toLowerCase(),
    identifierKind: IdentifierKind.Ethereum,
  });
  return inboxId ?? null;
};
