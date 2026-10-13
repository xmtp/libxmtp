import { Badge, Button, Group, Stack, Text, TextInput } from "@mantine/core";
import { useCallback } from "react";

import { Member } from "@/components/Conversation/Member";
import type { MemberProfile } from "@/helpers/member";
import { useMemberId } from "@/hooks/useMemberId";

export type RemoveMembersProps = {
  existingMembers: MemberProfile[];
  clientInboxId: string;
  removedMembers: MemberProfile[];
  onMembersRemoved?: (members: MemberProfile[]) => void;
};

export const RemoveMembers: React.FC<RemoveMembersProps> = ({
  existingMembers,
  clientInboxId,
  removedMembers,
  onMembersRemoved,
}) => {
  const {
    memberId,
    setMemberId,
    inboxId,
    error: lookupError,
    loading,
  } = useMemberId();
  const member = existingMembers.find(
    (candidate) => candidate.inboxId === inboxId,
  );
  let error = lookupError;
  if (!error && inboxId) {
    if (!member) {
      error = "This inbox is not a member of this group";
    } else if (inboxId === clientInboxId) {
      error = "You cannot remove yourself here";
    } else if (member.permissionLevel === "superAdmin") {
      error = "Remove this member's super admin role first";
    } else if (
      removedMembers.some((candidate) => candidate.inboxId === inboxId)
    ) {
      error = "This member is already in the removed list";
    }
  }
  const canRemove = !!member && !error && !loading;
  const handleRemoveMember = () => {
    if (!canRemove) return;
    onMembersRemoved?.([...removedMembers, member]);
    setMemberId("");
  };

  const handleRestoreRemovedMember = useCallback(
    (inboxId: string) => {
      const newRemovedMembers = removedMembers.filter(
        (m) => m.inboxId !== inboxId,
      );
      if (newRemovedMembers.length === removedMembers.length) {
        return;
      }
      onMembersRemoved?.(newRemovedMembers);
    },
    [onMembersRemoved, removedMembers],
  );

  return (
    <>
      <Group gap="xs" align="flex-start">
        <TextInput
          flex={1}
          size="sm"
          label="Remove by address, inbox ID, or handle"
          value={memberId}
          error={error}
          onChange={(event) => setMemberId(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              handleRemoveMember();
            }
          }}
        />
        <Button
          size="sm"
          mt="24px"
          disabled={!canRemove}
          loading={loading}
          onClick={handleRemoveMember}>
          Remove
        </Button>
      </Group>
      <Group gap="xs" justify="space-between" align="center">
        <Text fw={700}>Removed members</Text>
        <Badge color="gray" size="lg">
          {removedMembers.length}
        </Badge>
      </Group>
      {removedMembers.length > 0 && (
        <Stack gap="4px">
          {removedMembers.map((member) => (
            <Member
              key={member.inboxId}
              buttonLabel="Restore"
              address={member.address}
              displayName={member.displayName}
              avatar={member.avatar}
              description={member.description}
              handle={
                existingMembers.find(
                  (existing) => existing.inboxId === member.inboxId,
                )?.handle ??
                member.handle ??
                null
              }
              inboxId={member.inboxId}
              onClick={() => {
                handleRestoreRemovedMember(member.inboxId);
              }}
            />
          ))}
        </Stack>
      )}
    </>
  );
};
