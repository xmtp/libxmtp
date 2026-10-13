import { Box, Card, Flex, Stack, Text } from "@mantine/core";
import { useMemo } from "react";
import { useNavigate, useParams } from "react-router";

import { HandleText } from "@/components/Handles/HandleText";
import { useConversation } from "@/hooks/useConversation";

import styles from "./ConversationCard.module.css";

export type ConversationCardProps = {
  conversationId: string;
};

export const ConversationCard: React.FC<ConversationCardProps> = ({
  conversationId,
}) => {
  const { name, handle, members } = useConversation(conversationId);
  const navigate = useNavigate();
  const { conversationId: paramsConversationId } = useParams();

  const memberCount = useMemo(() => {
    return members.size;
  }, [members]);

  return (
    <Box px="sm">
      <Card
        shadow="sm"
        padding="sm"
        radius="md"
        withBorder
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            void navigate(`/conversations/${conversationId}`);
          }
        }}
        onClick={() => void navigate(`/conversations/${conversationId}`)}
        className={[
          styles.root,
          conversationId === paramsConversationId && styles.selected,
        ].join(" ")}>
        <Stack gap="0">
          <Flex align="center">
            <Text fw={700} truncate>
              {handle ? (
                <HandleText handle={handle} withPrefix inherit />
              ) : (
                name || "Untitled"
              )}
            </Text>
          </Flex>
          <Text size="sm">
            {memberCount} member{memberCount !== 1 ? "s" : ""}
          </Text>
        </Stack>
      </Card>
    </Box>
  );
};
