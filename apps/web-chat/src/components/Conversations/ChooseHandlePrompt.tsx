import { Box, Card, CloseButton, Group, Stack, Text } from "@mantine/core";
import { useLocalStorage } from "@mantine/hooks";
import { useNavigate } from "react-router";

import { useClient } from "@/contexts/XMTPContext";
import { useOwnHandles } from "@/hooks/useOwnHandles";

import styles from "./HelpCard.module.css";

/** Asks a user with no handle to choose one, until the user dismisses it. */
export const ChooseHandlePrompt: React.FC = () => {
  const client = useClient();
  const navigate = useNavigate();
  const { handle, loaded } = useOwnHandles();
  const [dismissed, setDismissed] = useLocalStorage<boolean>({
    key: `XMTP_HANDLE_PROMPT_DISMISSED_${client.inboxId}`,
    defaultValue: false,
    getInitialValueInEffect: false,
  });

  if (!loaded || handle !== null || dismissed) {
    return null;
  }

  const handleOpen = () => {
    void navigate("/identity");
  };

  return (
    <Box px="sm" pt="sm" className={styles.container}>
      <Card
        shadow="sm"
        padding="sm"
        radius="md"
        withBorder
        tabIndex={0}
        onKeyDown={(e) => {
          // a key on the Dismiss button bubbles here; it must not open the page
          if (e.key === "Enter" && e.target === e.currentTarget) {
            handleOpen();
          }
        }}
        onClick={handleOpen}
        className={styles.root}>
        <Group justify="space-between" align="flex-start" wrap="nowrap">
          <Stack gap="0">
            <Text fw={700} truncate>
              Choose a handle
            </Text>
            <Text size="sm" c="dimmed">
              People on this server can use it to find you and send you
              messages.
            </Text>
          </Stack>
          <CloseButton
            size="sm"
            aria-label="Dismiss"
            onClick={(e) => {
              e.stopPropagation();
              setDismissed(true);
            }}
          />
        </Group>
      </Card>
    </Box>
  );
};
