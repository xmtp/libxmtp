import { Button, NativeSelect, Stack, Text } from "@mantine/core";
import { Storage, XmtpError } from "@xmtp/browser-sdk";
import { useState } from "react";

import { Modal } from "@/components/Modal";
import { backendLabel } from "@/helpers/backend";
import { useSettings } from "@/hooks/useSettings";

export const LocalDatabases: React.FC = () => {
  const { backendUrl } = useSettings();
  const [opened, setOpened] = useState(false);
  const [files, setFiles] = useState<string[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();

  const manage = async (remove = false) => {
    setLoading(true);
    setError(undefined);
    try {
      const label = await backendLabel(backendUrl);
      const admin = await Storage.admin();
      try {
        const available = (await admin.listFiles()).filter(
          (file) =>
            (file.replace(/^\/+/, "").startsWith(`xmtp-sdk/${label}/`) &&
              file.endsWith("/xmtp.db3")) ||
            (file.split("/").at(-1)?.startsWith(`xmtp-${label}-`) &&
              file.endsWith(".db3")),
        );
        if (remove) {
          if (!selected || !available.includes(selected))
            throw new Error("Select a local database.");
          await admin.deleteFile(selected);
          setSelected(null);
        }
        setFiles(
          remove ? available.filter((file) => file !== selected) : available,
        );
      } finally {
        await admin.end();
      }
    } catch (cause) {
      setError(
        cause instanceof XmtpError.StorageBusy
          ? "Another tab uses XMTP storage. Close that tab, then try again."
          : cause instanceof Error
            ? cause.message
            : "Unable to read local databases.",
      );
    } finally {
      setLoading(false);
    }
  };
  return (
    <>
      <Button
        variant="subtle"
        onClick={() => {
          setOpened(true);
          void manage();
        }}>
        Local databases
      </Button>
      <Modal
        opened={opened}
        onClose={() => setOpened(false)}
        title="Local databases">
        <Stack>
          <Text size="sm">
            Select a database for this backend. Deleting it removes its local
            messages.
          </Text>
          <NativeSelect
            label="Database"
            data={[
              { value: "", label: "Select a database" },
              ...files.map((file) => ({ value: file, label: file })),
            ]}
            value={selected ?? ""}
            onChange={(event) => setSelected(event.currentTarget.value || null)}
          />
          {error && <Text c="red">{error}</Text>}
          <Button
            color="red"
            disabled={!selected}
            loading={loading}
            onClick={() => void manage(true)}>
            Delete selected database
          </Button>
        </Stack>
      </Modal>
    </>
  );
};
