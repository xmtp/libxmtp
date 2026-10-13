import { Button, Group, Stack, Text } from "@mantine/core";

import { Modal } from "@/components/Modal";

export type ConfirmHandleChangeProps = {
  current: string;
  next: string | null;
  onCancel: () => void;
  onConfirm: (next: string) => void;
};

/** Asks before a claim replaces the current handle. */
export const ConfirmHandleChange: React.FC<ConfirmHandleChangeProps> = ({
  current,
  next,
  onCancel,
  onConfirm,
}) => {
  return (
    <Modal
      opened={next !== null}
      onClose={onCancel}
      centered
      title={<Text fw={700}>Change your handle?</Text>}>
      <Stack gap="md">
        <Text size="sm">
          People will see {next} as your handle. {current} stays yours, and
          people can still find you by it. You cannot change your handle again
          for 7 days.
        </Text>
        <Group justify="flex-end">
          <Button variant="default" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            onClick={() => {
              if (next !== null) onConfirm(next);
            }}>
            Change
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
};
