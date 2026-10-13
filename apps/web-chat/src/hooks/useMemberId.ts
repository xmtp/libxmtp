import { useEffect, useState } from "react";

import { isValidHandle } from "@/helpers/handles";
import {
  getInboxIdForAddressQuery,
  getInboxIdForHandleQuery,
} from "@/helpers/inboxId";
import { isValidEthereumAddress, isValidInboxId } from "@/helpers/strings";
import { useSettings } from "@/hooks/useSettings";

// a handle is typed one character at a time; look it up when typing pauses
const HANDLE_LOOKUP_DELAY_MS = 300;

type Lookup = {
  // the input and backend that the lookup answers
  memberId: string;
  backendUrl: string;
  inboxId: string;
  address: string;
  handle: string | null;
  error: string | null;
};

const lookUp = async (
  memberId: string,
  backendUrl: string,
): Promise<Omit<Lookup, "memberId" | "backendUrl">> => {
  const empty = { inboxId: "", address: "", handle: null, error: null };
  if (isValidEthereumAddress(memberId)) {
    try {
      const inboxId = await getInboxIdForAddressQuery(
        memberId.toLowerCase(),
        backendUrl,
      );
      return inboxId
        ? { ...empty, inboxId, address: memberId }
        : { ...empty, error: "Address not registered on XMTP" };
    } catch {
      return {
        ...empty,
        error: "Unable to get inbox ID for address. Try again.",
      };
    }
  }
  try {
    const result = await getInboxIdForHandleQuery(memberId, backendUrl);
    return result
      ? { ...empty, inboxId: result.inboxId, handle: result.handle }
      : { ...empty, error: "No inbox has this handle on this server" };
  } catch {
    return { ...empty, error: "Unable to get inbox ID for handle. Try again." };
  }
};

/**
 * Resolves what the user typed: an Ethereum address, an inbox ID, or a
 * handle. A handle resolves through the SDK, which checks the answer against
 * the inbox's identity log (IDENT-085).
 */
export const useMemberId = () => {
  const [memberId, setMemberId] = useState("");
  const [lookup, setLookup] = useState<Lookup>();
  const { backendUrl } = useSettings();

  const isAddress = isValidEthereumAddress(memberId);
  const isHandle = !isAddress && isValidHandle(memberId);
  const needsLookup = isAddress || isHandle;

  useEffect(() => {
    if (!needsLookup) return;
    // a lookup for older input must not replace the result for newer input
    let current = true;
    const timeout = setTimeout(
      () => {
        void lookUp(memberId, backendUrl).then((result) => {
          if (current) setLookup({ memberId, backendUrl, ...result });
        });
      },
      isHandle ? HANDLE_LOOKUP_DELAY_MS : 0,
    );
    return () => {
      current = false;
      clearTimeout(timeout);
    };
  }, [backendUrl, isHandle, memberId, needsLookup]);

  const answered =
    lookup?.memberId === memberId && lookup.backendUrl === backendUrl
      ? lookup
      : undefined;

  let inboxId = "";
  let error: string | null = null;
  if (!memberId) {
    // nothing typed
  } else if (needsLookup) {
    inboxId = answered?.inboxId ?? "";
    error = answered?.error ?? null;
  } else if (isValidInboxId(memberId)) {
    inboxId = memberId;
  } else {
    error = "Invalid address, inbox ID, or handle";
  }

  return {
    memberId,
    setMemberId,
    error,
    loading: needsLookup && !answered,
    inboxId,
    address: answered?.address ?? "",
    handle: answered?.handle ?? null,
    displayName: null,
    description: null,
    avatar: null,
  };
};
