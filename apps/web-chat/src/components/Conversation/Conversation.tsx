import { ActionIcon, Group, Text, Tooltip } from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import { Group as XmtpGroup } from "@xmtp/browser-sdk";
import { useCallback, useEffect } from "react";
import { Outlet, useNavigate } from "react-router";

import { ConversationMenu } from "@/components/Conversation/ConversationMenu";
import { MembersList } from "@/components/Conversation/MembersList";
import { HandleText } from "@/components/Handles/HandleText";
import { Messages } from "@/components/Messages/Messages";
import { ConversationProvider } from "@/contexts/ConversationContext";
import { useClient } from "@/contexts/XMTPContext";
import { useConsentState } from "@/hooks/useConsentState";
import { useConversation } from "@/hooks/useConversation";
import { IconInfo } from "@/icons/IconInfo";
import { IconUsers } from "@/icons/IconUsers";
import { ContentLayout } from "@/layouts/ContentLayout";

import { Composer } from "./Composer";

export type ConversationProps = {
  conversationId: string;
};

export const Conversation: React.FC<ConversationProps> = ({
  conversationId,
}) => {
  const [opened, { toggle }] = useDisclosure();
  const navigate = useNavigate();
  const {
    conversation,
    name,
    handle,
    sync,
    loading: conversationLoading,
    messages,
    syncing: conversationSyncing,
  } = useConversation(conversationId);
  const consentState = useConsentState(useClient(), conversation);

  useEffect(() => {
    const loadMessages = async () => {
      await sync(true);
    };
    void loadMessages();
  }, [sync]);

  const handleSync = useCallback(async () => {
    await sync(true);
  }, [sync]);

  return (
    <>
      <ConversationProvider
        key={conversationId}
        conversationId={conversationId}
        consentState={consentState}>
        <ContentLayout
          title={
            handle ? (
              <HandleText handle={handle} withPrefix inherit />
            ) : (
              name || "Untitled"
            )
          }
          loading={messages.length === 0 && conversationLoading}
          headerActions={
            <Group gap="xxs">
              <Tooltip label="Conversation Details" fz="xs">
                <ActionIcon
                  variant="default"
                  onClick={() =>
                    void navigate(`/conversations/${conversationId}/details`)
                  }>
                  <IconInfo />
                </ActionIcon>
              </Tooltip>
              <ConversationMenu
                conversationId={conversationId}
                type={conversation instanceof XmtpGroup ? "group" : "dm"}
                onSync={() => void handleSync()}
                disabled={conversationSyncing}
              />
              <Tooltip
                label={
                  opened ? (
                    <Text size="xs">Hide members</Text>
                  ) : (
                    <Text size="xs">Show members</Text>
                  )
                }>
                <ActionIcon
                  variant="default"
                  onClick={() => {
                    toggle();
                  }}>
                  <IconUsers />
                </ActionIcon>
              </Tooltip>
            </Group>
          }
          aside={
            <MembersList conversationId={conversationId} toggle={toggle} />
          }
          asideOpened={opened}
          footer={<Composer conversationId={conversationId} />}
          withScrollArea={false}>
          <Messages messages={messages} />
        </ContentLayout>
      </ConversationProvider>
      <Outlet context={{ conversationId }} />
    </>
  );
};
