import { Box, Button, Group, Text } from "@mantine/core";

import { Modal } from "@/components/Modal";
import { useCollapsedMediaQuery } from "@/hooks/useCollapsedMediaQuery";
import { ContentLayout } from "@/layouts/ContentLayout";

export type AppLockDisconnectModalProps = {
  opened: boolean;
  onClose: () => void;
};

export const AppLockDisconnectModal: React.FC<AppLockDisconnectModalProps> = ({
  opened,
  onClose,
}) => {
  const fullScreen = useCollapsedMediaQuery();
  const contentHeight = fullScreen ? "auto" : 500;
  const footer = (
    <Group justify="flex-end" flex={1} p="md">
      <Button onClick={onClose}>OK</Button>
    </Group>
  );

  return (
    <Modal
      closeOnClickOutside={false}
      closeOnEscape={false}
      withCloseButton={false}
      opened={opened}
      centered
      fullScreen={fullScreen}
      onClose={onClose}
      size="sm"
      padding={0}>
      <ContentLayout
        title="xmtp.chat session takeover requested"
        maxHeight={contentHeight}
        footer={footer}
        withScrollFade={false}
        withScrollAreaPadding={false}>
        <Box p="md">
          <Text mb="md">
            This tab has taken the app lock. Wait for the other tab to close its
            client, then connect.
          </Text>
        </Box>
      </ContentLayout>
    </Modal>
  );
};
