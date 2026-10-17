import type {
  PublicIdentity,
  KeyPackageStatus,
  Installation as XmtpInstallation,
} from "@xmtp/browser-sdk";
import { useCallback, useEffect, useRef, useState } from "react";

import { useClient, useXMTP } from "@/contexts/XMTPContext";

export type Installation = XmtpInstallation & {
  keyPackageStatus: KeyPackageStatus | undefined;
};

type Identity = {
  inboxId: string | null;
  recoveryIdentity: PublicIdentity | null;
  identities: PublicIdentity[];
  installations: Installation[];
};

const EMPTY_IDENTITY: Identity = {
  inboxId: null,
  recoveryIdentity: null,
  identities: [],
  installations: [],
};

export const useIdentity = (syncOnMount: boolean = false) => {
  const client = useClient();
  const { signer } = useXMTP();
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
      const inboxState = await client.inboxState(true);
      const installations = inboxState.installations.toSorted((a, b) => {
        if ((a.createdAt?.ns ?? 0n) > (b.createdAt?.ns ?? 0n)) {
          return -1;
        } else if ((a.createdAt?.ns ?? 0n) < (b.createdAt?.ns ?? 0n)) {
          return 1;
        }
        return 0;
      });
      const keyPackageStatuses = await client.keyPackageStatuses(
        installations.map((installation) => installation.id),
      );
      identity = {
        inboxId: inboxState.inboxId,
        identities: inboxState.identities,
        recoveryIdentity: inboxState.recoveryIdentity,
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

  const revokeInstallation = async (installationId: string) => {
    setRevoking(true);

    try {
      if (!signer) throw new Error("Wallet signer not available");
      await client.revokeInstallations(signer, [installationId]);
    } finally {
      setRevoking(false);
    }
  };

  const revokeAllOtherInstallations = async () => {
    setRevoking(true);

    try {
      if (!signer) throw new Error("Wallet signer not available");
      await client.revokeAllOtherInstallations(signer);
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
