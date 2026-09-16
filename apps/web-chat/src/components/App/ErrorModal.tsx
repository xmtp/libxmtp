import { Box, Button, Group, Tabs, Text } from "@mantine/core";
import { Opfs } from "@xmtp/browser-sdk";
import { useEffect, useState } from "react";
import { CodeWithCopy } from "@/components/CodeWithCopy";
import { Modal } from "@/components/Modal";
import { useCollapsedMediaQuery } from "@/hooks/useCollapsedMediaQuery";
import { ContentLayout } from "@/layouts/ContentLayout";
import { backendLabel } from "@/helpers/backend";
import { useSettings } from "@/hooks/useSettings";

export const ErrorModal: React.FC = () => {
  const [unhandledRejectionError, setUnhandledRejectionError] =
    useState<Error | null>(null);
  const fullScreen = useCollapsedMediaQuery();
  const contentHeight = fullScreen ? "auto" : 500;
  const { backendUrl } = useSettings();
  const [deletionError, setDeletionError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const canDeleteDatabase =
    unhandledRejectionError?.message.startsWith(
      "[StorageError::PreTransitionDatabase]",
    ) ||
    unhandledRejectionError?.message.startsWith(
      "[StorageError::OldStreamDatabase]",
    );

  const deleteLocalDatabase = async () => {
    setDeleting(true);
    setDeletionError(null);
    const label = await backendLabel(backendUrl);
    const prefix = `xmtp-${label}-`;
    const opfs = await Opfs.create();
    try {
      const matchingFiles = (await opfs.listFiles()).filter(
        (file) => file.startsWith(prefix) && file.endsWith(".db3"),
      );
      await Promise.all(matchingFiles.map((file) => opfs.deleteFile(file)));
      let survivors = (await opfs.listFiles()).filter(
        (file) => file.startsWith(prefix) && file.endsWith(".db3"),
      );
      if (survivors.length > 0) {
        await Promise.all(survivors.map((file) => opfs.deleteFile(file)));
        survivors = (await opfs.listFiles()).filter(
          (file) => file.startsWith(prefix) && file.endsWith(".db3"),
        );
      }
      if (survivors.length > 0) {
        setDeletionError(`Unable to delete: ${survivors.join(", ")}`);
      } else {
        setUnhandledRejectionError(null);
      }
    } finally {
      opfs.close();
      setDeleting(false);
    }
  };

  useEffect(() => {
    const handleUnhandledRejection = (event: PromiseRejectionEvent) => {
      setUnhandledRejectionError(event.reason as Error);
    };
    const handleBoundaryError = (event: CustomEvent<Error>) => {
      setUnhandledRejectionError(event.detail);
    };
    window.addEventListener("unhandledrejection", handleUnhandledRejection);
    window.addEventListener(
      "errorboundary",
      handleBoundaryError as EventListener,
    );
    return () => {
      window.removeEventListener(
        "unhandledrejection",
        handleUnhandledRejection,
      );
      window.removeEventListener(
        "errorboundary",
        handleBoundaryError as EventListener,
      );
    };
  }, []);

  const footer = (
    <Group justify="space-between" flex={1} p="md">
      <Button
        variant="default"
        component="a"
        href="https://github.com/xmtp/libxmtp/issues/new/choose"
        target="_blank"
      >
        Report issue
      </Button>
      {canDeleteDatabase && (
        <Button
          color="red"
          loading={deleting}
          onClick={() => void deleteLocalDatabase()}
        >
          Delete local database
        </Button>
      )}
      <Button
        onClick={() => {
          setUnhandledRejectionError(null);
        }}
      >
        OK
      </Button>
    </Group>
  );

  return unhandledRejectionError ? (
    <Modal
      opened={!!unhandledRejectionError}
      onClose={() => {
        setUnhandledRejectionError(null);
      }}
      fullScreen={fullScreen}
      closeOnEscape={false}
      closeOnClickOutside={false}
      withCloseButton={false}
      padding={0}
      centered
    >
      <ContentLayout
        title="Application error"
        maxHeight={contentHeight}
        footer={footer}
        withScrollFade={false}
        withScrollAreaPadding={false}
      >
        <Box p="md">
          <Tabs defaultValue="message">
            <Tabs.List>
              <Tabs.Tab value="message">Message</Tabs.Tab>
              <Tabs.Tab value="stackTrace">Stack trace</Tabs.Tab>
            </Tabs.List>
            <Tabs.Panel value="message" py="md">
              <CodeWithCopy code={unhandledRejectionError.message} />
              {deletionError && (
                <Text c="red" mt="md">
                  {deletionError}
                </Text>
              )}
            </Tabs.Panel>
            <Tabs.Panel value="stackTrace" py="md">
              <CodeWithCopy
                code={
                  unhandledRejectionError.stack ?? "Stack trace not available"
                }
              />
            </Tabs.Panel>
          </Tabs>
        </Box>
      </ContentLayout>
    </Modal>
  ) : null;
};
