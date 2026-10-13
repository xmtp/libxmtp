import { Badge, Button, Group, Stack, Text, TextInput } from "@mantine/core";
import { useCallback } from "react";

import { Member } from "@/components/Conversation/Member";
import type { Member as MemberCardMember } from "@/components/Conversation/MemberCard";
import type { MemberProfile } from "@/helpers/member";
import { useMemberId } from "@/hooks/useMemberId";

const hasInboxId = (
  members: MemberProfile[] | PendingMember[],
  inboxId: string,
) => {
  return members.some((member) => member.inboxId === inboxId);
};

export type PendingMember = MemberCardMember;

export type AddMembersProps = {
  existingMembers: MemberProfile[];
  addedMembers: PendingMember[];
  onMembersAdded?: (members: PendingMember[]) => void;
};

export const AddMembers: React.FC<AddMembersProps> = ({
  existingMembers,
  addedMembers,
  onMembersAdded,
}) => {
  const {
    loading,
    memberId,
    setMemberId,
    displayName: memberIdDisplayName,
    error: memberIdError,
    inboxId: memberIdInboxId,
    address: memberIdAddress,
    description: memberIdDescription,
    avatar: memberIdAvatar,
    handle: memberIdHandle,
  } = useMemberId();
  const error =
    memberIdInboxId &&
    (hasInboxId(existingMembers, memberIdInboxId) ||
      hasInboxId(addedMembers, memberIdInboxId))
      ? "This member is already in the list"
      : null;

  const handleAddMember = useCallback(() => {
    if (hasInboxId(addedMembers, memberIdInboxId)) return;
    const member = {
      inboxId: memberIdInboxId,
      address: memberIdAddress,
      displayName: memberIdDisplayName,
      avatar: memberIdAvatar,
      description: memberIdDescription,
      handle: memberIdHandle,
    };
    setMemberId("");
    onMembersAdded?.([...addedMembers, member]);
  }, [
    addedMembers,
    memberIdInboxId,
    memberIdAddress,
    memberIdDisplayName,
    memberIdDescription,
    memberIdAvatar,
    memberIdHandle,
    onMembersAdded,
    setMemberId,
  ]);

  const handleRemoveAddedMember = useCallback(
    (inboxId: string) => {
      const newAddedMembers = addedMembers.filter((m) => m.inboxId !== inboxId);
      onMembersAdded?.(newAddedMembers);
    },
    [addedMembers, onMembersAdded],
  );

  return (
    <>
      <Group gap="xs" align="flex-start">
        <TextInput
          flex={1}
          size="sm"
          label="Address, inbox ID, or handle"
          styles={{
            label: {
              marginBottom: "var(--mantine-spacing-xxs)",
            },
          }}
          error={memberIdError || error}
          value={memberId}
          onChange={(event) => {
            setMemberId(event.target.value);
          }}
          onKeyDown={(event) => {
            if (event.key === " ") {
              event.preventDefault();
              return;
            }
            if (
              event.key === "Enter" &&
              memberIdError === null &&
              error === null &&
              !loading &&
              memberIdInboxId
            ) {
              handleAddMember();
            }
          }}
        />
        <Button
          size="sm"
          mt="32px"
          disabled={
            memberIdError !== null ||
            loading ||
            !memberIdInboxId ||
            error !== null
          }
          loading={loading}
          onClick={handleAddMember}>
          Add
        </Button>
      </Group>
      <Stack gap="xs">
        <Group gap="xs" justify="space-between" align="center">
          <Text fw={700}>Added members</Text>
          <Badge color="gray" size="lg">
            {addedMembers.length}
          </Badge>
        </Group>
        {addedMembers.length > 0 && (
          <Stack gap="4px">
            {addedMembers.map((member) => (
              <Member
                key={member.inboxId}
                displayName={member.displayName}
                address={member.address}
                avatar={member.avatar}
                description={member.description}
                handle={member.handle}
                inboxId={member.inboxId}
                onClick={() => {
                  handleRemoveAddedMember(member.inboxId);
                }}
              />
            ))}
          </Stack>
        )}
      </Stack>
    </>
  );
};
