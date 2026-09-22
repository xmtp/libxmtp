import type {
  Identifier,
  KeyPackageStatus,
  Installation as XmtpInstallation,
} from "@xmtp/browser-sdk";
import { useCallback, useEffect, useRef, useState } from "react";

import { useClient } from "@/contexts/XMTPContext";

export type Installation = XmtpInstallation & {
  keyPackageStatus: KeyPackageStatus | undefined;
};

type Identity = {
  inboxId: string | null;
  recoveryIdentifier: Identifier | null;
  accountIdentifiers: Identifier[];
  installations: Installation[];
};

const EMPTY_IDENTITY: Identity = {
  inboxId: null,
  recoveryIdentifier: null,
  accountIdentifiers: [],
  installations: [],
};

export const useIdentity = (syncOnMount: boolean = false) => {
  const client = useClient();
  const [refreshingClient, setRefreshingClient] = useState<typeof client>();
  const [revoking, setRevoking] = useState(false);
  const [result, setResult] = useState<{
    client: typeof client;
    identity: Identity;
  }>();
  const generation = useRef(0);

  const loadIdentity = useCallback(async () => {
    const request = ++generation.current;
    let identity: Identity | undefined;
    try {
      const inboxState = await client.preferences.fetchInboxState();
      const installations = inboxState.installations.toSorted((a, b) => {
        if (a.clientTimestampNs! > b.clientTimestampNs!) {
          return -1;
        } else if (a.clientTimestampNs! < b.clientTimestampNs!) {
          return 1;
        }
        return 0;
      });
      const keyPackageStatuses = await client.fetchKeyPackageStatuses(
        installations.map((installation) => installation.id),
      );
      identity = {
        inboxId: inboxState.inboxId,
        accountIdentifiers: inboxState.accountIdentifiers,
        recoveryIdentifier: inboxState.recoveryIdentifier,
        installations: installations.map((installation) => ({
          ...installation,
          keyPackageStatus: keyPackageStatuses.get(installation.id),
        })),
      };
    } finally {
      if (request === generation.current) {
        setResult((current) => ({
          client,
          identity:
            identity ??
            (current?.client === client ? current.identity : EMPTY_IDENTITY),
        }));
      }
    }
  }, [client]);

  useEffect(() => {
    if (syncOnMount) {
      void loadIdentity().catch(console.error);
    }
    return () => {
      generation.current += 1;
    };
  }, [loadIdentity, syncOnMount]);

  const sync = useCallback(async () => {
    setRefreshingClient(client);
    try {
      await loadIdentity();
    } finally {
      setRefreshingClient((current) =>
        current === client ? undefined : current,
      );
    }
  }, [client, loadIdentity]);

  const revokeInstallation = async (installationIdBytes: Uint8Array) => {
    setRevoking(true);

    try {
      await client.revokeInstallations([installationIdBytes]);
    } finally {
      setRevoking(false);
    }
  };

  const revokeAllOtherInstallations = async () => {
    setRevoking(true);

    try {
      await client.revokeAllOtherInstallations();
    } finally {
      setRevoking(false);
    }
  };

  return {
    ...(result?.client === client ? result.identity : EMPTY_IDENTITY),
    revokeAllOtherInstallations,
    revokeInstallation,
    revoking,
    sync,
    syncing:
      refreshingClient === client || (syncOnMount && result?.client !== client),
  };
};
