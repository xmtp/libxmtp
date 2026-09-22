import { Button, Group, Stack, Text } from "@mantine/core";
import { useMemo } from "react";
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
  const requestedUrl = searchParams.get("backend") ?? "";
  const opened =
    isValidBackendUrl(requestedUrl) &&
    (!isValidBackendUrl(backendUrl) ||
      new URL(requestedUrl).origin !== new URL(backendUrl).origin);
  const requestedHost = useMemo(
    () => (isValidBackendUrl(requestedUrl) ? backendHost(requestedUrl) : ""),
    [requestedUrl],
  );

  const close = () =>
    void navigate(window.location.pathname, { replace: true });

  return (
    <Modal opened={opened} onClose={close} title="Switch backend?" centered>
      <Stack>
        <Text>
          Disconnect and switch xmtp.chat to <strong>{requestedHost}</strong>?
        </Text>
        <Group justify="flex-end">
          <Button variant="default" onClick={close}>
            Cancel
          </Button>
          <Button
            onClick={() => {
              disconnect();
              setBackendUrl(requestedUrl);
              close();
            }}>
            Switch backend
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
};
