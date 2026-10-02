import {
  Client,
  type CredentialSource,
  type LogLevel,
  initLogging,
  XmtpError,
  type Signer,
} from "@xmtp/browser-sdk";
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
} from "react";

import { backendLabel } from "@/helpers/backend";
import { useAppLock, type AppLockState } from "@/hooks/useAppLock";
import { useActions } from "@/stores/inbox/hooks";

export type InitializeClientOptions = {
  authCallback?: CredentialSource;
  backendUrl: string;
  env?: string;
  loggingLevel?: LogLevel;
  signer: Signer;
};

export type XMTPContextValue = {
  /**
   * The XMTP client instance
   */
  client?: Client;
  signer?: Signer;
  /**
   * Set the XMTP client instance
   */
  setClient: React.Dispatch<React.SetStateAction<Client | undefined>>;
  initialize: (options: InitializeClientOptions) => Promise<Client | undefined>;
  initializing: boolean;
  error: Error | null;
  disconnect: () => Promise<void>;
  lockState: AppLockState;
  acquireLock: () => void;
  releaseLock: () => void;
};

export const XMTPContext = createContext<XMTPContextValue>({
  setClient: () => {},
  initialize: () => Promise.reject(new Error("XMTPProvider not available")),
  initializing: false,
  error: null,
  disconnect: async () => {},
  lockState: "available",
  acquireLock: () => false,
  releaseLock: () => {},
});

export type XMTPProviderProps = React.PropsWithChildren & {
  /**
   * Initial XMTP client instance
   */
  client?: Client;
};

export const XMTPProvider: React.FC<XMTPProviderProps> = ({
  children,
  client: initialClient,
}) => {
  const { reset } = useActions();
  const [client, setClient] = useState<Client | undefined>(initialClient);
  const [clientSigner, setClientSigner] = useState<Signer>();
  // when another session claims the lock, disconnect without releasing
  const handleLockLost = useCallback(async () => {
    if (client) {
      await client.end();
      setClientSigner(undefined);
      setClient(undefined);
      reset();
    }
  }, [client, reset]);
  const { lockState, acquireLock, releaseLock } = useAppLock(() => {
    void handleLockLost();
  });
  const [initializing, setInitializing] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  // client is initializing
  const initializingRef = useRef(false);

  /**
   * Initialize an XMTP client
   */
  const initialize = useCallback(
    async ({
      authCallback,
      backendUrl,
      env,
      loggingLevel,
      signer,
    }: InitializeClientOptions) => {
      // only initialize a client if one doesn't already exist
      if (!client) {
        const lockAcquired = acquireLock();
        // if the client is already initializing or the lock can't be acquired,
        // don't do anything
        if (initializingRef.current || !lockAcquired) {
          return undefined;
        }

        // flag the client as initializing
        initializingRef.current = true;

        // reset error state
        setError(null);
        // reset initializing state
        setInitializing(true);

        let xmtpClient: Client;

        try {
          // create a new XMTP client
          await initLogging({ level: loggingLevel ?? "warn" });
          xmtpClient = await Client.create(signer, {
            backend: {
              url: backendUrl,
              credentials: authCallback,
              appVersion: "xmtp.chat/0",
            },
            storage: {
              location: "default",
              label: env ?? (await backendLabel(backendUrl)),
            },
          });
          setClientSigner(signer);
          setClient(xmtpClient);
        } catch (e) {
          setClient(undefined);
          const error =
            e instanceof XmtpError.StorageBusy
              ? new Error(
                  "Another tab uses XMTP storage. Close that tab, then connect again.",
                )
              : (e as Error);
          setError(error);
          // release lock on error
          releaseLock();
          // re-throw error for upstream consumption
          throw error;
        } finally {
          initializingRef.current = false;
          setInitializing(false);
        }

        return xmtpClient;
      }
      return client;
    },
    [client, acquireLock, releaseLock],
  );

  const disconnect = useCallback(async () => {
    if (client) {
      await client.end();
      setClient(undefined);
      setClientSigner(undefined);
      reset();
      releaseLock();
    }
  }, [client, setClient, releaseLock, reset]);

  // memo-ize the context value to prevent unnecessary re-renders
  const value = useMemo(
    () => ({
      client,
      signer: clientSigner,
      setClient,
      initialize,
      initializing,
      error,
      disconnect,
      lockState,
      acquireLock,
      releaseLock,
    }),
    [
      client,
      clientSigner,
      initialize,
      initializing,
      error,
      disconnect,
      lockState,
      acquireLock,
      releaseLock,
    ],
  );

  return <XMTPContext.Provider value={value}>{children}</XMTPContext.Provider>;
};

export const useXMTP = () => {
  return useContext(XMTPContext);
};

export const useClient = () => {
  const { client } = useXMTP();
  if (!client) {
    throw new Error("useClient: XMTP client not initialized");
  }
  return client;
};
