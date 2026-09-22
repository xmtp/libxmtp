import { Group, Stack, Text, Textarea, TextInput } from "@mantine/core";
import { Group as XmtpGroup, type Conversation } from "@xmtp/browser-sdk";
import { useState } from "react";

import { type ClientPermissions } from "@/hooks/useClientPermissions";

type MetadataProps = {
  conversation?: Conversation;
  clientPermissions?: ClientPermissions;
  onNameChange: (name: string) => void;
  onDescriptionChange: (description: string) => void;
  onImageUrlChange: (imageUrl: string) => void;
};

export const Metadata: React.FC<MetadataProps> = ({
  conversation,
  clientPermissions,
  onNameChange,
  onDescriptionChange,
  onImageUrlChange,
}) => {
  const [name, setName] = useState(
    conversation instanceof XmtpGroup ? (conversation.name ?? "") : "",
  );
  const [description, setDescription] = useState(
    conversation instanceof XmtpGroup ? (conversation.description ?? "") : "",
  );
  const [imageUrl, setImageUrl] = useState(
    conversation instanceof XmtpGroup ? (conversation.imageUrl ?? "") : "",
  );

  return (
    <Stack gap="xs" p="md">
      <Group gap="sm" align="center" wrap="nowrap">
        <Text flex="1 1 20%" size="sm">
          Name
        </Text>
        <TextInput
          size="sm"
          flex="1 1 65%"
          aria-label="Name"
          value={name}
          disabled={
            conversation &&
            clientPermissions &&
            !clientPermissions.canChangeGroupName
          }
          onChange={(event) => {
            const value = event.target.value;
            setName(value);
            onNameChange(value);
          }}
        />
      </Group>
      <Group gap="sm" align="flex-start" wrap="nowrap">
        <Text flex="1 1 20%" size="sm">
          Description
        </Text>
        <Textarea
          size="sm"
          flex="1 1 65%"
          aria-label="Description"
          value={description}
          disabled={
            conversation &&
            clientPermissions &&
            !clientPermissions.canChangeGroupDescription
          }
          onChange={(event) => {
            const value = event.target.value;
            setDescription(value);
            onDescriptionChange(value);
          }}
        />
      </Group>
      <Group gap="sm" align="center" wrap="nowrap">
        <Text flex="1 1 20%" size="sm">
          Image URL
        </Text>
        <TextInput
          size="sm"
          flex="1 1 65%"
          aria-label="Image URL"
          value={imageUrl}
          disabled={
            conversation &&
            clientPermissions &&
            !clientPermissions.canChangeGroupImage
          }
          onChange={(event) => {
            const value = event.target.value;
            setImageUrl(value);
            onImageUrlChange(value);
          }}
        />
      </Group>
    </Stack>
  );
};
