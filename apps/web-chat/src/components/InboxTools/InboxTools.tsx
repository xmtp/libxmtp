import {
  Button,
  Group,
  Stack,
  Stepper,
  Text,
  TextInput,
  Title,
} from "@mantine/core";
import {
  Client,
  type AuthCallback,
  type Installation,
  type Signer,
} from "@xmtp/browser-sdk";
import { useCallback, useMemo, useRef, useState } from "react";
import { Outlet } from "react-router";
import { useSignMessage } from "wagmi";

import { BackendUrlInput } from "@/components/App/BackendUrlInput";
import { ConnectedAddress } from "@/components/App/ConnectedAddress";
import { WalletConnect } from "@/components/App/WalletConnect";
import { InstallationTable } from "@/components/InboxTools/InstallationTable";
import { useAuthToken } from "@/contexts/AuthTokenContext";
import { backendLabel } from "@/helpers/backend";
import { createEOASigner, createSCWSigner } from "@/helpers/createSigner";
import { isValidInboxId } from "@/helpers/strings";
import { useEphemeralSigner } from "@/hooks/useEphemeralSigner";
import { useMemberId } from "@/hooks/useMemberId";
import { useSettings } from "@/hooks/useSettings";
import { useWallet } from "@/hooks/useWallet";
import { ContentLayout } from "@/layouts/ContentLayout";

type InboxToolsData = {
  installations: Installation[];
  inboxUpdatesCount: number | null;
  selectedInstallationIds: string[];
  loading: boolean;
};

const EMPTY_DATA: InboxToolsData = {
  installations: [],
  inboxUpdatesCount: null,
  selectedInstallationIds: [],
  loading: false,
};

export const InboxTools: React.FC = () => {
  const {
    address,
    isConnected,
    disconnect,
    loading: walletLoading,
  } = useWallet();
  const { address: ephemeralAddress, signer: ephemeralSigner } =
    useEphemeralSigner();
  const { signMessageAsync } = useSignMessage();
  const {
    inboxId,
    memberId,
    setMemberId,
    error: memberIdError,
  } = useMemberId();
  const {
    blockchain,
    backendUrl,
    useSCW,
    ephemeralAccountEnabled,
    setEphemeralAccountEnabled,
  } = useSettings();
  const query = useMemo(() => ({ backendUrl, inboxId }), [backendUrl, inboxId]);
  const [result, setResult] = useState<
    InboxToolsData & { query: typeof query }
  >();
  const { installations, inboxUpdatesCount, selectedInstallationIds, loading } =
    result?.query === query ? result : EMPTY_DATA;
  const updateQuery = useCallback(
    (update: Partial<InboxToolsData>) => {
      setResult((current) => ({
        ...(current?.query === query ? current : EMPTY_DATA),
        query,
        ...update,
      }));
    },
    [query],
  );
  const finishQuery = useCallback(
    (update: Partial<InboxToolsData>) => {
      // A response from an older inbox or backend cannot replace a newer query.
      setResult((current) =>
        current?.query === query ? { ...current, ...update } : current,
      );
    },
    [query],
  );
  const selectInstallations = useCallback(
    (ids: React.SetStateAction<string[]>) => {
      setResult((current) =>
        current?.query === query
          ? {
              ...current,
              selectedInstallationIds:
                typeof ids === "function"
                  ? ids(current.selectedInstallationIds)
                  : ids,
            }
          : current,
      );
    },
    [query],
  );
  const { createAuthCallback } = useAuthToken();
  // The inbox tools statics build their own short-lived clients, separate from
  // the app's client, so they get their own callback and their own memo.
  const authCallbackRef = useRef<AuthCallback | null>(null);
  authCallbackRef.current ??= createAuthCallback();
  const authCallback = authCallbackRef.current;
  const [active, setActive] = useState(1);

  const fetchInstallations = useCallback(async () => {
    const inboxState = await Client.fetchInboxStates([inboxId], {
      authCallback,
      backendUrl,
      env: await backendLabel(backendUrl),
    });
    return inboxState[0].installations.toSorted(
      (a, b) =>
        Number(b.clientTimestampNs ?? 0) - Number(a.clientTimestampNs ?? 0),
    );
  }, [inboxId, backendUrl, authCallback]);

  const handleFindInstallations = useCallback(async () => {
    if (!isValidInboxId(inboxId)) {
      return;
    }
    updateQuery({
      loading: true,
      installations: [],
      selectedInstallationIds: [],
    });
    try {
      finishQuery({ installations: await fetchInstallations() });
    } catch (error) {
      console.error(error);
    } finally {
      finishQuery({ loading: false });
    }
  }, [inboxId, updateQuery, finishQuery, fetchInstallations]);

  const handleFetchInboxUpdatesCount = useCallback(async () => {
    if (!isValidInboxId(inboxId)) {
      return;
    }
    updateQuery({ loading: true, inboxUpdatesCount: null });
    try {
      const inboxUpdatesCounts = await Client.fetchLatestInboxUpdatesCount(
        [inboxId],
        { authCallback, backendUrl, env: await backendLabel(backendUrl) },
      );
      finishQuery({ inboxUpdatesCount: inboxUpdatesCounts.get(inboxId) ?? 0 });
    } catch (error) {
      console.error(error);
    } finally {
      finishQuery({ loading: false });
    }
  }, [inboxId, backendUrl, authCallback, updateQuery, finishQuery]);

  const handleRevokeInstallations = useCallback(
    async (installationIds: Uint8Array[]) => {
      let signer: Signer;
      if (ephemeralAccountEnabled) {
        if (!ephemeralAddress) {
          console.error("Ephemeral wallet not connected");
          return;
        }
        signer = ephemeralSigner;
      } else {
        if (!address) {
          console.error("Wallet not connected");
          return;
        }
        if (useSCW && blockchain <= 0) {
          console.error("Smart contract wallet chain ID not set");
          return;
        }
        signer = useSCW
          ? createSCWSigner(
              address,
              (message: string) => signMessageAsync({ message }),
              blockchain,
            )
          : createEOASigner(address, (message: string) =>
              signMessageAsync({ message }),
            );
      }
      updateQuery({ loading: true });
      try {
        await Client.revokeInstallations(signer, inboxId, installationIds, {
          authCallback,
          backendUrl,
          env: await backendLabel(backendUrl),
        });
        finishQuery({ selectedInstallationIds: [] });
        finishQuery({ installations: await fetchInstallations() });
      } catch (error) {
        console.error(error);
      } finally {
        finishQuery({ loading: false });
      }
    },
    [
      authCallback,
      backendUrl,
      address,
      blockchain,
      useSCW,
      signMessageAsync,
      inboxId,
      fetchInstallations,
      updateQuery,
      finishQuery,
      ephemeralAccountEnabled,
      ephemeralAddress,
      ephemeralSigner,
    ],
  );

  const handleDisconnectWallet = useCallback(() => {
    if (isConnected) {
      disconnect();
    } else {
      setEphemeralAccountEnabled(false);
    }
    setMemberId("");
    setResult(undefined);
  }, [isConnected, disconnect, setEphemeralAccountEnabled, setMemberId]);

  return (
    <>
      <ContentLayout
        loading={loading}
        title={
          <Group justify="space-between" align="center" flex={1}>
            <Text size="lg" fw={700} c="text.primary">
              Installation management
            </Text>
          </Group>
        }
        footer={
          <Group justify="flex-end" p="md" flex={1}>
            {!(address || ephemeralAccountEnabled) && (
              <Text size="sm" c="dimmed">
                Wallet connection required
              </Text>
            )}
            <Button
              disabled={selectedInstallationIds.length === 0}
              onClick={() => {
                const installationBytes = installations
                  .filter((installation) =>
                    selectedInstallationIds.includes(installation.id),
                  )
                  .map((installation) => installation.bytes);
                void handleRevokeInstallations(installationBytes);
              }}>
              Revoke installations
            </Button>
          </Group>
        }>
        <Stepper active={active} onStepClick={setActive} mt="md">
          <Stepper.Step
            label="Connect your wallet"
            allowStepSelect={false}
            loading={walletLoading}>
            <WalletConnect />
          </Stepper.Step>
          <Stepper.Step label="Manage installations" allowStepSelect={false}>
            <Stack gap="md" py="md">
              {address || ephemeralAccountEnabled ? (
                <Group justify="space-between" align="center">
                  <ConnectedAddress
                    size="sm"
                    address={address ?? ephemeralAddress}
                    onClick={handleDisconnectWallet}
                  />
                  <BackendUrlInput />
                </Group>
              ) : (
                <Group justify="space-between" align="flex-start">
                  <WalletConnect />
                  <BackendUrlInput />
                </Group>
              )}
              <Stack gap="xs" mb="md">
                <Group justify="space-between" align="center">
                  <Text size="sm" pl={4}>
                    Enter an address or inbox ID
                  </Text>
                  {memberIdError && (
                    <Text c="red.7" size="sm">
                      {memberIdError}
                    </Text>
                  )}
                </Group>
                <TextInput
                  size="sm"
                  error={!!memberIdError}
                  value={memberId}
                  onChange={(event) => {
                    setMemberId(event.target.value);
                  }}
                />
                <Group justify="space-between" align="center">
                  <Button
                    variant="default"
                    onClick={() => {
                      setMemberId(address ?? ephemeralAddress);
                    }}>
                    Use wallet address
                  </Button>
                  <Group gap="xs">
                    <Button
                      variant="default"
                      disabled={!isValidInboxId(inboxId)}
                      onClick={() => {
                        void handleFetchInboxUpdatesCount();
                      }}>
                      Check updates count
                    </Button>
                    <Button
                      disabled={!isValidInboxId(inboxId)}
                      onClick={() => {
                        void handleFindInstallations();
                      }}>
                      Find installations
                    </Button>
                  </Group>
                </Group>
              </Stack>
              <Title order={4}>Inbox updates count</Title>
              <Text>
                {inboxUpdatesCount === null
                  ? "No count fetched"
                  : inboxUpdatesCount.toString()}
              </Text>
              <Title order={4}>Installations</Title>
              <Stack gap="md">
                {installations.length === 0 && (
                  <Text>No installations found</Text>
                )}
                {installations.length > 0 && (
                  <InstallationTable
                    installations={installations}
                    selectedInstallationIds={selectedInstallationIds}
                    setSelectedInstallationIds={selectInstallations}
                  />
                )}
              </Stack>
            </Stack>
          </Stepper.Step>
        </Stepper>
      </ContentLayout>
      <Outlet />
    </>
  );
};
