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

import {
  cleanAttachmentDirectory,
  pendingAttachmentCleanupPaths,
  retryPendingDatabaseDeletions,
} from "@/helpers/attachment";
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
  const [client, setClientState] = useState<Client | undefined>(initialClient);
  const clientRef = useRef<Client | undefined>(initialClient);
  const setClient = useCallback<
    React.Dispatch<React.SetStateAction<Client | undefined>>
  >((next) => {
    const updated = typeof next === "function" ? next(clientRef.current) : next;
    clientRef.current = updated;
    setClientState(updated);
  }, []);
  const [clientSigner, setClientSigner] = useState<Signer>();
  const [error, setError] = useState<Error | null>(null);
  const lockLossEpoch = useRef(0);
  const attachmentDbPath = useRef<string | undefined>(undefined);
  const endedClient = useRef<Client | undefined>(undefined);
  // when another session claims the lock, disconnect without releasing
  const handleLockLost = useCallback(async () => {
    lockLossEpoch.current += 1;
    const current = clientRef.current;
    if (current) {
      clientRef.current = undefined;
      const dbPath = attachmentDbPath.current;
      try {
        await current.end();
      } catch (cause) {
        setError(cause instanceof Error ? cause : new Error(String(cause)));
      }
      try {
        await cleanAttachmentDirectory(dbPath);
        attachmentDbPath.current = undefined;
      } catch (cause) {
        setError(cause instanceof Error ? cause : new Error(String(cause)));
      } finally {
        endedClient.current = undefined;
        setClientSigner(undefined);
        setClient(undefined);
        reset();
      }
    }
  }, [reset, setClient]);
  const { lockState, acquireLock, releaseLock, ownsLock } = useAppLock(() => {
    void handleLockLost();
  });
  const [initializing, setInitializing] = useState(false);
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
        const startingLockEpoch = lockLossEpoch.current;

        // reset error state
        setError(null);
        // reset initializing state
        setInitializing(true);

        let xmtpClient: Client;

        try {
          await retryPendingDatabaseDeletions();
          const pendingPaths = new Set([
            attachmentDbPath.current,
            ...pendingAttachmentCleanupPaths(),
          ]);
          for (const dbPath of pendingPaths) {
            await cleanAttachmentDirectory(dbPath);
          }
          attachmentDbPath.current = undefined;
          // create a new XMTP client
          await initLogging({ level: loggingLevel ?? "warn" });
          const backend = {
            url: backendUrl,
            credentials: authCallback,
            appVersion: "xmtp.chat/0",
          };
          const label = env ?? (await backendLabel(backendUrl));
          if (lockLossEpoch.current !== startingLockEpoch || !ownsLock()) {
            throw new Error("App lock was lost during XMTP initialization");
          }
          xmtpClient = await Client.create(signer, {
            backend,
            storage: {
              location: "default",
              label,
            },
          });
          let dbPath: string | undefined;
          try {
            dbPath = await xmtpClient.storage.path();
          } catch (cause) {
            await xmtpClient.end();
            throw cause;
          }
          if (lockLossEpoch.current !== startingLockEpoch || !ownsLock()) {
            await xmtpClient.end();
            throw new Error("App lock was lost during XMTP initialization");
          }
          attachmentDbPath.current = dbPath;
          endedClient.current = undefined;
          setClientSigner(signer);
          setClient(xmtpClient);
        } catch (e) {
          setClient(undefined);
          setClientSigner(undefined);
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
    [client, acquireLock, ownsLock, releaseLock, setClient],
  );

  const disconnect = useCallback(async () => {
    if (client) {
      const dbPath = attachmentDbPath.current ?? (await client.storage.path());
      attachmentDbPath.current = dbPath;
      if (endedClient.current !== client) {
        await client.end();
        endedClient.current = client;
      }
      await cleanAttachmentDirectory(dbPath);
      attachmentDbPath.current = undefined;
      endedClient.current = undefined;
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
      setClient,
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
