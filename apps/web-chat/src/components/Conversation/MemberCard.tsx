import { Avatar, Card, Group, Stack, Text } from "@mantine/core";
import { forwardRef } from "react";

import { useMemberPopover } from "@/components/Conversation/MemberPopover";
import { shortAddress } from "@/helpers/strings";

import classes from "./MemberCard.module.css";

export type Member = {
  address: string;
  displayName: string | null;
  avatar: string | null;
  description: string | null;
  // the current handle identifies the inbox when it has one (IDENT-087)
  handle: string | null;
  inboxId: string;
};

export type MemberCardProps = Member & {
  withClass?: boolean;
  shortenAddress?: boolean;
};

export const MemberCard = forwardRef<HTMLDivElement, MemberCardProps>(
  (
    {
      address,
      displayName,
      avatar,
      description,
      handle,
      inboxId,
      withClass = true,
      shortenAddress = true,
    },
    ref,
  ) => {
    const { setOpened } = useMemberPopover();
    const shownInboxId = shortenAddress ? shortAddress(inboxId) : inboxId;
    return (
      <Card
        ref={ref}
        shadow="sm"
        px="xxxs"
        py="xxs"
        radius="md"
        withBorder
        onClick={() => {
          setOpened((o) => !o);
        }}
        className={withClass ? classes.member : undefined}
        tabIndex={0}>
        <Group gap="xxs" align="center" wrap="nowrap">
          <Avatar src={avatar} size="md" radius="xl" variant="default" />
          <Stack gap="0" flex={1} style={{ overflow: "hidden" }}>
            <Text size="sm" truncate>
              {handle ||
                displayName ||
                (shortenAddress ? shortAddress(address) : address)}
            </Text>
            <Text size="xs" truncate c="dimmed">
              {handle ? shownInboxId : description}
            </Text>
          </Stack>
        </Group>
      </Card>
    );
  },
);

MemberCard.displayName = "MemberCard";
