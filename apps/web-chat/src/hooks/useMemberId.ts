import { useEffect, useState } from "react";

import { getInboxIdForAddressQuery } from "@/helpers/inboxId";
import { isValidEthereumAddress, isValidInboxId } from "@/helpers/strings";
import { useSettings } from "@/hooks/useSettings";

export const useMemberId = () => {
  const [loading, setLoading] = useState(false);
  const [memberId, setMemberId] = useState("");
  const [inboxId, setInboxId] = useState("");
  const [address, setAddress] = useState("");
  const [displayName, setDisplayName] = useState<string | null>(null);
  const [description, setDescription] = useState<string | null>(null);
  const [avatar, setAvatar] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { backendUrl } = useSettings();

  useEffect(() => {
    const checkMemberId = async () => {
      setError(null);
      setInboxId("");
      setAddress("");
      setDisplayName(null);
      setDescription(null);
      setAvatar(null);

      if (!memberId) {
        return;
      }

      if (!isValidEthereumAddress(memberId) && !isValidInboxId(memberId)) {
        setError("Invalid address or inbox ID");
      } else if (isValidEthereumAddress(memberId)) {
        setLoading(true);

        try {
          const inboxId = await getInboxIdForAddressQuery(
            memberId.toLowerCase(),
            backendUrl,
          );

          if (!inboxId) {
            setError("Address not registered on XMTP");
          } else {
            setInboxId(inboxId);
            setAddress(memberId);
          }
        } catch {
          setError("Unable to get inbox ID for address. Try again.");
        } finally {
          setLoading(false);
        }
      } else if (isValidInboxId(memberId)) {
        setInboxId(memberId);
      }
    };

    void checkMemberId();
  }, [memberId, backendUrl]);

  return {
    memberId,
    setMemberId,
    error,
    loading,
    inboxId,
    address,
    displayName,
    description,
    avatar,
    setMemberIdError: setError,
  };
};
