import type { AuthCallback } from "@xmtp/browser-sdk";
import { useCallback, useEffect, useRef } from "react";
import { useNavigate } from "react-router";
import { hexToUint8Array } from "uint8array-extras";
import { useAccount, useSignMessage } from "wagmi";

import { useAuthToken } from "@/contexts/AuthTokenContext";
import { useXMTP } from "@/contexts/XMTPContext";
import { isValidBackendUrl } from "@/helpers/backend";
import { createEOASigner, createSCWSigner } from "@/helpers/createSigner";
import { useEphemeralSigner } from "@/hooks/useEphemeralSigner";
import { useSettings } from "@/hooks/useSettings";

export const useConnectXmtp = () => {
  const navigate = useNavigate();
  const { signer: ephemeralSigner } = useEphemeralSigner();
  const { initializing, client, initialize, lockState } = useXMTP();
  const { createAuthCallback } = useAuthToken();
  // One callback for this app's client, kept for the hook's lifetime so its
  // memo of offered tokens matches that client's credential cache.
  const authCallbackRef = useRef<AuthCallback | null>(null);
  authCallbackRef.current ??= createAuthCallback();
  const authCallback = authCallbackRef.current;
  const account = useAccount();
  const { signMessageAsync } = useSignMessage();
  const {
    blockchain,
    encryptionKey,
    backendUrl,
    ephemeralAccountEnabled,
    loggingLevel,
    useSCW,
    autoConnect,
    setAutoConnect,
  } = useSettings();

  const connect = useCallback(() => {
    // if client is already connected or lock is not available, return
    if (client || lockState !== "available") {
      return;
    }

    // Client.create throws "backendUrl is required" on an empty URL, which
    // surfaces as an unhandled rejection in the application error modal. The
    // deployed app ships with no default backend, so guard here as well as in
    // the disabled Connect button: a stored autoConnect with a cleared URL
    // reaches this path without a click.
    if (!isValidBackendUrl(backendUrl)) {
      setAutoConnect(false);
      return;
    }

    // connect ephemeral account if enabled
    if (ephemeralAccountEnabled) {
      initialize({
        authCallback,
        backendUrl,
        dbEncryptionKey: encryptionKey
          ? hexToUint8Array(encryptionKey)
          : undefined,
        loggingLevel,
        signer: ephemeralSigner,
      })
        .then(() => {
          setAutoConnect(true);
        })
        .catch((error: unknown) => {
          // disable auto connect on error to prevent retry loop
          setAutoConnect(false);
          throw error;
        });
      return;
    }

    // if wallet is not connected or SCW is enabled but chain is not set, return
    if (!account.address || (useSCW && blockchain <= 0)) {
      return;
    }

    initialize({
      authCallback,
      backendUrl,
      dbEncryptionKey: encryptionKey
        ? hexToUint8Array(encryptionKey)
        : undefined,
      loggingLevel,
      signer: useSCW
        ? createSCWSigner(
            account.address,
            (message: string) => signMessageAsync({ message }),
            blockchain,
          )
        : createEOASigner(account.address, (message: string) =>
            signMessageAsync({ message }),
          ),
    })
      .then(() => {
        setAutoConnect(true);
      })
      .catch((error: unknown) => {
        // disable auto connect on error to prevent retry loop
        setAutoConnect(false);
        throw error;
      });
  }, [
    account.address,
    authCallback,
    client,
    blockchain,
    encryptionKey,
    backendUrl,
    ephemeralAccountEnabled,
    ephemeralSigner,
    initialize,
    lockState,
    loggingLevel,
    setAutoConnect,
    signMessageAsync,
    useSCW,
  ]);

  useEffect(() => {
    if (client) {
      void navigate("/conversations");
    } else if (autoConnect) {
      connect();
    }
  }, [client, navigate, autoConnect, connect]);

  return {
    client,
    loading: initializing,
    connect,
  };
};
