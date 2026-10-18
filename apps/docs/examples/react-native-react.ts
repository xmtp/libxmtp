import type { Client, Signer } from "@xmtp/react-native-sdk";
import { XmtpProvider, useClient, useXmtp } from "@xmtp/react-native-sdk/react";
import { createElement, type ReactNode } from "react";

// #region provider
export function MessagingProvider({ children }: { children: ReactNode }) {
  return createElement(XmtpProvider, { children });
}
// #endregion provider

// #region initialize
export function useMessaging(
  signer: Signer,
  backendUrl: string,
  databaseKey: Uint8Array,
) {
  const { client, error, isLoading, initialize, disconnect } = useClient();
  return {
    client,
    error,
    isLoading,
    connect: () =>
      initialize({
        signer,
        options: {
          backend: { url: backendUrl },
          storage: { location: "default", encryptionKey: databaseKey },
        },
      }),
    disconnect,
  };
}
// #endregion initialize

// #region borrowed
export function ExistingClientProvider({
  client,
  children,
}: {
  client: Client;
  children: ReactNode;
}) {
  return createElement(XmtpProvider, { client, children });
}

export function useCurrentMessagingClient() {
  return useXmtp().client;
}
// #endregion borrowed
