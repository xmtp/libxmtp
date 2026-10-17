import type { PublicIdentity } from "@xmtp/browser-sdk";
import { useEffect, useState } from "react";
import { useLocation, useNavigate, useParams } from "react-router";

import { LoadingMessage } from "@/components/LoadingMessage";
import { useClient } from "@/contexts/XMTPContext";
import { isValidHandle } from "@/helpers/handles";
import {
  asciiLowercase,
  isValidEthereumAddress,
  normalizeMemberInput,
} from "@/helpers/strings";
import { useActions } from "@/stores/inbox/hooks";

const REDIRECT_TIMEOUT = 2000;

export const LoadDM: React.FC = () => {
  const [message, setMessage] = useState("");
  const { address } = useParams();
  // the code of a handle link such as /dm/nick#2341 is the URL fragment
  const { hash } = useLocation();
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

    const resolveIdentity = (address: string): PublicIdentity | null => {
      if (asciiLowercase(address).startsWith("0x")) {
        return isValidEthereumAddress(asciiLowercase(address))
          ? { identifier: asciiLowercase(address), kind: "ethereum" }
          : null;
      }
      // keep the code, or refuse a fragment that is not one: never resolve
      // the bare name in place of the handle in the link. Lowercase ASCII
      // only, as the protocol does: a folded U+212A would resolve "kate".
      const handle = normalizeMemberInput(`${address}${hash}`);
      return isValidHandle(handle)
        ? { identifier: handle, kind: "handle" }
        : null;
    };

    const loadDm = async () => {
      if (!address) {
        navigateToHome("No address, redirecting...");
        return;
      }

      try {
        setMessage("Resolving address...");
        const identity = resolveIdentity(address);

        if (!identity) {
          navigateToHome("Invalid identifier, redirecting...");
          return;
        }

        const notFound =
          identity.kind === "handle"
            ? "No inbox has this handle on this server, redirecting..."
            : "Address not registered on the XMTP network, redirecting...";
        if (identity.kind === "ethereum") {
          setMessage("Verifying address...");
          const reachable = await client.canMessage([identity]);

          if (!reachable.get(`ethereum:${identity.identifier}`)) {
            navigateToHome(notFound);
            return;
          }
        }

        // the SDK checks a handle answer against the inbox's identity log
        const inboxId = await client.inboxIdFor(identity);
        if (!inboxId) {
          navigateToHome(notFound);
          return;
        }

        const dm = await client.conversations.getDmByInboxId(inboxId);
        let dmId = dm?.id;
        if (!dmId) {
          // no DM group, create it
          setMessage("Creating new DM...");
          const newDm = await client.conversations.createDm(inboxId);
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
  }, [addConversation, address, client, hash, navigate]);

  return <LoadingMessage message={message} />;
};
