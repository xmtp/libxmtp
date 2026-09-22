import { Box, Button, Group, TextInput } from "@mantine/core";
import { useCallback, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { Modal } from "@/components/Modal";
import { useCollapsedMediaQuery } from "@/hooks/useCollapsedMediaQuery";
import { useConversations } from "@/hooks/useConversations";
import { useMemberId } from "@/hooks/useMemberId";
import { ContentLayout } from "@/layouts/ContentLayout";
import { useActions } from "@/stores/inbox/hooks";

export const CreateDmModal: React.FC = () => {
  const { createDm } = useConversations();
  const { addConversation } = useActions();
  const [loading, setLoading] = useState(false);
  const {
    memberId,
    setMemberId,
    error: memberIdError,
    inboxId,
  } = useMemberId();
  const navigate = useNavigate();
  const fullScreen = useCollapsedMediaQuery();
  const contentHeight = fullScreen ? "auto" : 500;

  const handleClose = useCallback(() => {
    void navigate(-1);
  }, [navigate]);

  const handleCreate = useCallback(async () => {
    setLoading(true);

    try {
      const conversation = await createDm(inboxId);
      // ensure conversation is added to store so navigation works
      await addConversation(conversation);
      void navigate(`/conversations/${conversation.id}`);
    } finally {
      setLoading(false);
    }
  }, [createDm, addConversation, navigate, inboxId]);

  const footer = useMemo(() => {
    return (
      <Group justify="flex-end" flex={1} p="md">
        <Button variant="default" onClick={handleClose}>
          Cancel
        </Button>
        <Button
          variant="filled"
          disabled={loading || memberIdError !== null || !inboxId}
          loading={loading}
          onClick={() => void handleCreate()}
        >
          Create
        </Button>
      </Group>
    );
  }, [handleClose, handleCreate, inboxId, loading, memberIdError]);

  return (
    <Modal
      closeOnClickOutside={false}
      closeOnEscape={false}
      withCloseButton={false}
      opened
      centered
      fullScreen={fullScreen}
      onClose={handleClose}
      size="600"
      padding={0}
    >
      <ContentLayout
        title="Create direct message"
        maxHeight={contentHeight}
        footer={footer}
        withScrollAreaPadding={false}
      >
        <Box p="md">
          <TextInput
            size="sm"
            label="Address or inbox ID"
            styles={{
              label: {
                marginBottom: "var(--mantine-spacing-xxs)",
              },
            }}
            error={memberIdError}
            value={memberId}
            onChange={(event) => {
              setMemberId(event.target.value.toLowerCase().replace(/\s/g, ""));
            }}
            onKeyDown={(event) => {
              if (event.key === " ") {
                event.preventDefault();
              }
            }}
          />
        </Box>
      </ContentLayout>
    </Modal>
  );
};
