import { Button, Group, Stack, Text } from "@mantine/core";
import { useMemo, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";

import { Modal } from "@/components/Modal";
import { useXMTP } from "@/contexts/XMTPContext";
import { backendHost, isValidBackendUrl } from "@/helpers/backend";
import { useSettings } from "@/hooks/useSettings";

export const SwitchBackendModal: React.FC = () => {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const { disconnect } = useXMTP();
  const { backendUrl, setBackendUrl } = useSettings();
  const [switching, setSwitching] = useState(false);
  const switchingRef = useRef(false);
  const [error, setError] = useState<string | null>(null);
  const requestedUrl = searchParams.get("backend") ?? "";
  const opened =
    isValidBackendUrl(requestedUrl) &&
    (!isValidBackendUrl(backendUrl) ||
      new URL(requestedUrl).origin !== new URL(backendUrl).origin);
  const requestedHost = useMemo(
    () => (isValidBackendUrl(requestedUrl) ? backendHost(requestedUrl) : ""),
    [requestedUrl],
  );

  const clearRequest = () => {
    setError(null);
    void navigate(window.location.pathname, { replace: true });
  };
  const close = () => {
    if (switchingRef.current) return;
    clearRequest();
  };

  const switchBackend = async () => {
    if (switchingRef.current) return;
    switchingRef.current = true;
    setError(null);
    setSwitching(true);
    try {
      await disconnect();
      setBackendUrl(requestedUrl);
      clearRequest();
    } catch {
      setError("Could not disconnect from XMTP. Try again.");
    } finally {
      switchingRef.current = false;
      setSwitching(false);
    }
  };

  return (
    <Modal opened={opened} onClose={close} title="Switch backend?" centered>
      <Stack>
        <Text>
          Disconnect and switch xmtp.chat to <strong>{requestedHost}</strong>?
        </Text>
        {error && <Text c="red">{error}</Text>}
        <Group justify="flex-end">
          <Button variant="default" onClick={close} disabled={switching}>
            Cancel
          </Button>
          <Button onClick={() => void switchBackend()} loading={switching}>
            Switch backend
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
};
