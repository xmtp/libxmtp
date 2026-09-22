import { Button, Paper, Stack, type ButtonVariant } from "@mantine/core";
import {
  ActionStyle,
  type Action,
  type Actions,
  type Intent,
} from "@xmtp/browser-sdk";
import { isAfter } from "date-fns";
import { useCallback, useEffect, useState } from "react";

import BreakableText from "@/components/Messages/BreakableText";
import { useConversationContext } from "@/contexts/ConversationContext";
import { nsToDate } from "@/helpers/date";
import { useConversation } from "@/hooks/useConversation";

export type ActionsContentProps = {
  content: Actions;
};

const styleToVariantMap: Record<Required<Action>["style"], ButtonVariant> = {
  [ActionStyle.Primary]: "filled",
  [ActionStyle.Secondary]: "default",
  [ActionStyle.Danger]: "filled",
};

const styleToColorMap: Record<Required<Action>["style"], string | undefined> = {
  [ActionStyle.Primary]: undefined,
  [ActionStyle.Secondary]: undefined,
  [ActionStyle.Danger]: "red",
};

export const ActionsContent: React.FC<ActionsContentProps> = ({ content }) => {
  const { conversationId } = useConversationContext();
  const { sendIntent } = useConversation(conversationId);
  const [now, setNow] = useState(Date.now);

  useEffect(() => {
    const deadlines = content.actions.flatMap((action) => {
      const expiresAtNs = action.expiresAtNs || content.expiresAtNs;
      return expiresAtNs ? [nsToDate(expiresAtNs).getTime()] : [];
    });
    const nextDeadline = Math.min(...deadlines.filter((time) => time >= now));
    if (!Number.isFinite(nextDeadline)) return;

    // Update at the next expiration. Browsers limit a timeout to a signed int.
    const delay = Math.max(
      0,
      Math.min(nextDeadline - Date.now() + 1, 2_147_483_647),
    );
    const timeout = window.setTimeout(() => {
      setNow(Date.now());
    }, delay);
    return () => {
      window.clearTimeout(timeout);
    };
  }, [content, now]);
  const handleActionClick = useCallback(
    (actionId: string) => {
      const intent: Intent = {
        id: content.id,
        actionId,
      };
      void sendIntent(intent);
    },
    [sendIntent, content],
  );
  const actionsExpiration = content.expiresAtNs
    ? nsToDate(content.expiresAtNs)
    : undefined;
  return (
    <Paper p="sm" radius="md" withBorder>
      <Stack gap="xxs">
        <BreakableText>{content.description}</BreakableText>
        {content.actions.map((action) => {
          const actionExpiration = action.expiresAtNs
            ? nsToDate(action.expiresAtNs)
            : undefined;
          const expiration = actionExpiration ?? actionsExpiration;
          const isExpired = expiration && isAfter(now, expiration);
          return (
            <Button
              key={action.id}
              disabled={isExpired}
              title={isExpired ? "This action has expired" : undefined}
              variant={
                action.style ? styleToVariantMap[action.style] : "filled"
              }
              color={action.style ? styleToColorMap[action.style] : undefined}
              onClick={() => {
                if (!expiration || !isAfter(Date.now(), expiration)) {
                  handleActionClick(action.id);
                }
              }}>
              {action.label}
            </Button>
          );
        })}
      </Stack>
    </Paper>
  );
};
