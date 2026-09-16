import { IdentifierKind } from "@xmtp/browser-sdk";
import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router";
import { LoadingMessage } from "@/components/LoadingMessage";
import { useClient } from "@/contexts/XMTPContext";
import { isValidEthereumAddress } from "@/helpers/strings";
import { useActions } from "@/stores/inbox/hooks";

const REDIRECT_TIMEOUT = 2000;

export const LoadDM: React.FC = () => {
  const [message, setMessage] = useState("");
  const { address } = useParams();
  const { addConversation } = useActions();
  const navigate = useNavigate();
  const client = useClient();

  useEffect(() => {
    let timeout: ReturnType<typeof setTimeout>;

    const navigateToHome = (message: string) => {
      setMessage(message);
      timeout = setTimeout(() => {
        void navigate("/");
      }, REDIRECT_TIMEOUT);
    };

    const resolveAddress = (address: string) =>
      isValidEthereumAddress(address) ? address : null;

    const loadDm = async () => {
      if (!address) {
        navigateToHome("No address, redirecting...");
        return;
      }

      try {
        setMessage("Resolving address...");
        const resolvedAddress = resolveAddress(address);

        if (!resolvedAddress) {
          navigateToHome("Invalid identifier, redirecting...");
          return;
        }

        setMessage("Verifying address...");
        const inboxId = await client.fetchInboxIdByIdentifier({
          identifier: resolvedAddress,
          identifierKind: IdentifierKind.Ethereum,
        });

        if (!inboxId) {
          navigateToHome(
            "Address not registered on the XMTP network, redirecting...",
          );
          return;
        }

        const dm = await client.conversations.getDmByInboxId(inboxId);
        let dmId = dm?.id;
        if (!dmId) {
          // no DM group, create it
          setMessage("Creating new DM...");
          const newDm = await client.conversations.createDmWithIdentifier({
            identifier: resolvedAddress,
            identifierKind: IdentifierKind.Ethereum,
          });
          dmId = newDm.id;
          // add new DM to store
          await addConversation(newDm);
        }

        void navigate(`/conversations/${dmId}`);
      } catch (e) {
        console.error(e);

        navigateToHome("Error loading DM, redirecting...");

        // rethrow error for error modal
        throw e;
      }
    };

    void loadDm();

    return () => {
      clearTimeout(timeout);
    };
  }, [client, address]);

  return <LoadingMessage message={message} />;
};
