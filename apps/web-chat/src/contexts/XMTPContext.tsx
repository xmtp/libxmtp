import {
  Client,
  Storage,
  type BackendSource,
  type CredentialSource,
  type LogLevel,
  initLogging,
  type StorageLocation,
  XmtpError,
  type Signer,
} from "@xmtp/browser-sdk";
import { generateInboxId, initPureWasm } from "@xmtp/browser-sdk/pure";
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
} from "react";

import {
  cleanSessionAttachments,
  cleanStoredSessionAttachments,
  deploymentComponent,
  isCurrentDatabasePath,
  pendingAttachmentCleanupPaths,
  retryPendingDatabaseDeletions,
} from "@/helpers/attachment";
import { backendLabel } from "@/helpers/backend";
import { useAppLock, type AppLockState } from "@/hooks/useAppLock";
import { useActions } from "@/stores/inbox/hooks";

const storageLocation = async (
  signer: Signer,
  backend: BackendSource,
  label: string,
): Promise<StorageLocation> => {
  const admin = await Storage.admin();
  let files: string[];
  try {
    files = (await admin.listFiles()).map((path) => path.replace(/^\/+/, ""));
  } finally {
    await admin.end();
  }

  const prefix = `xmtp-${label}-`;
  const legacyFiles = files.filter(
    (path) =>
      !path.includes("/") &&
      path.startsWith(prefix) &&
      /^[0-9a-f]{64}\.db3$/i.test(path.slice(prefix.length)),
  );
  if (legacyFiles.length === 0) return "default";

  const identity = await signer.identity();
  const reachable = await Client.canMessage([identity], backend);
  const registered = reachable.get(`${identity.kind}:${identity.identifier}`);
  if (!registered) await initPureWasm();
  const inboxId = registered
    ? await Client.inboxIdFor(identity, backend)
    : generateInboxId(identity, 1n);
  const dbPath = legacyFiles.find(
    (path) =>
      path.slice(prefix.length).toLowerCase() ===
      `${inboxId}.db3`.toLowerCase(),
  );
  if (!dbPath) return "default";

  const currentCandidates = files.filter(
    (path) =>
      isCurrentDatabasePath(path, label) && path.split("/")[3] === inboxId,
  );
  const selectedDeploymentName = currentCandidates.length
    ? await deploymentComponent(
        (await Client.fetchServerConfiguration(backend)).identifier,
      )
    : undefined;
  const currentExists = currentCandidates.some((path) =>
    isCurrentDatabasePath(path, label, selectedDeploymentName),
  );
  if (currentExists)
    throw new Error("Both old and current databases match this inbox.");

  return { dbPath, attachmentsDir: `${dbPath}.attachments` };
};

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
  acquireLock: (force?: boolean) => boolean;
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
        await cleanSessionAttachments(dbPath);
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
  const handlePageHide = useCallback(async () => {
    const current = clientRef.current;
    let dbPath = attachmentDbPath.current;
    if (dbPath === undefined) {
      try {
        dbPath = await current?.storage.path();
      } catch {
        // End the client. The next startup scan can find its attachment path.
      }
    }
    await current?.end();
    await cleanSessionAttachments(dbPath);
  }, []);
  const { lockState, acquireLock, releaseLock, ownsLock } = useAppLock(() => {
    void handleLockLost();
  }, handlePageHide);
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
            ...(await pendingAttachmentCleanupPaths()),
          ]);
          for (const dbPath of pendingPaths) {
            await cleanSessionAttachments(dbPath);
          }
          await cleanStoredSessionAttachments();
          attachmentDbPath.current = undefined;
          // create a new XMTP client
          await initLogging({ level: loggingLevel ?? "warn" });
          const backend = {
            url: backendUrl,
            credentials: authCallback,
            appVersion: "xmtp.chat/0",
          };
          const label = env ?? (await backendLabel(backendUrl));
          const location = await storageLocation(signer, backend, label);
          if (lockLossEpoch.current !== startingLockEpoch || !ownsLock()) {
            throw new Error("App lock was lost during XMTP initialization");
          }
          xmtpClient = await Client.create(signer, {
            backend,
            registration: { nonce: location === "default" ? 0n : 1n },
            storage: {
              location,
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
      let dbPath = attachmentDbPath.current;
      if (dbPath === undefined) {
        try {
          dbPath = await client.storage.path();
        } catch {
          // End the client even if its storage worker cannot read the path.
        }
      }
      attachmentDbPath.current = dbPath;
      if (endedClient.current !== client) {
        await client.end();
        endedClient.current = client;
      }
      await cleanSessionAttachments(dbPath);
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
