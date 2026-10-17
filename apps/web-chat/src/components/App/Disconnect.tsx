import { Button, LoadingOverlay, Stack, Text } from "@mantine/core";
import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router";

import { useXMTP } from "@/contexts/XMTPContext";
import { useWallet } from "@/hooks/useWallet";
import { CenteredLayout } from "@/layouts/CenteredLayout";

export const Disconnect: React.FC = () => {
  const navigate = useNavigate();
  const { disconnect } = useWallet();
  const { disconnect: disconnectClient } = useXMTP();
  const [error, setError] = useState<string | null>(null);
  const [retrying, setRetrying] = useState(false);

  const closeClient = useCallback(async () => {
    setError(null);
    setRetrying(true);
    try {
      await disconnectClient();
      void navigate("/");
    } catch {
      setError("Could not disconnect from XMTP. Try again.");
    } finally {
      setRetrying(false);
    }
  }, [disconnectClient, navigate]);

  useEffect(() => {
    disconnect(() => {
      void closeClient();
    });
  }, [disconnect, closeClient]);

  return (
    <CenteredLayout>
      {error ? (
        <Stack>
          <Text c="red">{error}</Text>
          <Button loading={retrying} onClick={() => void closeClient()}>
            Retry
          </Button>
        </Stack>
      ) : (
        <LoadingOverlay visible={true} />
      )}
    </CenteredLayout>
  );
};
