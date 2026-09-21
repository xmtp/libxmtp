import {
  Alert,
  Button,
  Group,
  PasswordInput,
  Stack,
  Text,
} from "@mantine/core";
import { useEffect, useState } from "react";
import { Modal } from "@/components/Modal";
import { useAuthToken } from "@/contexts/AuthTokenContext";
import { useCollapsedMediaQuery } from "@/hooks/useCollapsedMediaQuery";
import { useSettings } from "@/hooks/useSettings";
import { ContentLayout } from "@/layouts/ContentLayout";

export const AuthTokenModal: React.FC = () => {
  const { request } = useAuthToken();
  const { authToken } = useSettings();
  const fullScreen = useCollapsedMediaQuery();
  const [value, setValue] = useState("");

  // Start from the stored token so a rejected one can be corrected rather than
  // retyped, and reset between prompts.
  useEffect(() => {
    if (request) {
      setValue(request.rejected ? authToken : "");
    }
  }, [request, authToken]);

  const submit = () => {
    if (value.trim() === "") return;
    request?.resolve(value);
  };

  const footer = (
    <Group justify="flex-end" flex={1} p="md">
      <Button disabled={value.trim() === ""} onClick={submit}>
        Use token
      </Button>
    </Group>
  );

  return request ? (
    <Modal
      opened
      // The backend holds its credential refresh lock while this is open, so
      // every pending request waits on it. Do not let it be dismissed into the
      // background with no token supplied.
      onClose={() => {}}
      fullScreen={fullScreen}
      closeOnEscape={false}
      closeOnClickOutside={false}
      withCloseButton={false}
      padding={0}
      centered
    >
      <ContentLayout
        title="Backend auth token"
        maxHeight={fullScreen ? "auto" : 320}
        footer={footer}
        withScrollFade={false}
        withScrollAreaPadding={false}
      >
        <Stack gap="md" p="md">
          {request.rejected && (
            <Alert color="red" title="Token rejected">
              The backend rejected the stored token, or it expired. Enter a
              current one to continue.
            </Alert>
          )}
          <Text size="sm">
            This backend requires a credential. It is sent to the backend you
            configured and is separate from your wallet signature.
          </Text>
          <PasswordInput
            aria-label="Backend auth token"
            data-autofocus
            value={value}
            placeholder="Paste the token"
            onChange={(event) => {
              setValue(event.currentTarget.value);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") submit();
            }}
          />
          <Text size="xs" c="dimmed">
            The <code>Bearer</code> prefix is added when the token does not
            already include a scheme.
          </Text>
        </Stack>
      </ContentLayout>
    </Modal>
  ) : null;
};
