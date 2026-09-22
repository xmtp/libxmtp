import { Box, Button, Group, Loader, Paper, Text } from "@mantine/core";
import { type RemoteAttachment } from "@xmtp/browser-sdk";
import { useEffect, useState } from "react";

import { AttachmentDetails } from "@/components/Messages/AttachmentDetails";
import type { MessageContentAlign } from "@/components/Messages/MessageContentWrapper";
import {
  downloadRemoteAttachment,
  formatFileSize,
  getFileType,
} from "@/helpers/attachment";

// Cache downloads, but let each mounted attachment own its object URL.
const attachmentCache = new Map<string, Promise<Blob>>();

const loadAttachment = (
  content: RemoteAttachment,
  key: string,
  force: boolean,
) => {
  const cached = attachmentCache.get(key);
  if (cached && !force) {
    return cached;
  }
  const download = downloadRemoteAttachment(content).then(
    (attachment) =>
      new Blob([attachment.content as Uint8Array<ArrayBuffer>], {
        type: attachment.mimeType,
      }),
  );
  attachmentCache.set(key, download);
  return download;
};

type AttachmentState =
  | { status: "loading" }
  | { status: "error" }
  | { status: "ready"; url: string };

export type RemoteAttachmentContentProps = {
  content: RemoteAttachment;
  align: MessageContentAlign;
};

export const RemoteAttachmentContent: React.FC<
  RemoteAttachmentContentProps
> = ({ content, align }) => {
  // Include the decryption data so a changed attachment gets fresh state.
  const attachmentKey = JSON.stringify(content);
  return (
    <AttachmentContent
      key={attachmentKey}
      attachmentKey={attachmentKey}
      content={content}
      align={align}
    />
  );
};

const AttachmentContent: React.FC<
  RemoteAttachmentContentProps & { attachmentKey: string }
> = ({ content, align, attachmentKey }) => {
  // The key replaces this snapshot when the attachment changes.
  const [source] = useState(content);
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<AttachmentState>({ status: "loading" });

  const handleRetry = () => {
    setState({ status: "loading" });
    setAttempt((value) => value + 1);
  };

  useEffect(() => {
    let active = true;
    let objectUrl: string | undefined;

    void loadAttachment(source, attachmentKey, attempt > 0)
      .then((blob) => {
        if (active) {
          objectUrl = URL.createObjectURL(blob);
          setState({ status: "ready", url: objectUrl });
        }
      })
      .catch(() => {
        if (active) {
          setState({ status: "error" });
        }
      });

    return () => {
      active = false;
      if (objectUrl) {
        URL.revokeObjectURL(objectUrl);
      }
    };
  }, [source, attachmentKey, attempt]);

  const fileSize = formatFileSize(content.contentLength);

  if (state.status === "loading") {
    return (
      <Paper p="sm" radius="md" withBorder>
        <Group gap="xs" align="center">
          <Loader size="sm" />
          <Text size="sm" c="dimmed">
            Loading attachment...
          </Text>
        </Group>
        <AttachmentDetails
          filename={content.filename ?? ""}
          fileSize={fileSize}
          align={align}
        />
      </Paper>
    );
  }

  if (state.status === "error") {
    return (
      <Paper p="sm" radius="md" withBorder>
        <Box>
          <Group gap="xs" align="center" mb="xs" wrap="nowrap">
            <Text size="sm" c="red" style={{ whiteSpace: "nowrap" }}>
              Unable to load attachment
            </Text>
            <Button size="xs" variant="light" onClick={handleRetry}>
              Retry
            </Button>
          </Group>
          <AttachmentDetails
            filename={content.filename ?? ""}
            fileSize={fileSize}
            align={align}
          />
        </Box>
      </Paper>
    );
  }

  const fileType = getFileType(content.filename ?? "");
  const decryptedUrl = state.url;

  return (
    <Paper p="sm" radius="md" withBorder>
      <Box>
        {fileType === "image" && (
          <img
            src={decryptedUrl}
            alt={content.filename || "Attachment"}
            style={{
              width: "100%",
              height: "auto",
              borderRadius: "var(--mantine-radius-sm)",
              objectFit: "contain",
              display: "block",
            }}
          />
        )}
        {fileType === "video" && (
          // oxlint-disable-next-line jsx-a11y/media-has-caption -- User-provided media does not include a captions track.
          <video
            src={decryptedUrl}
            controls
            style={{
              width: "100%",
              height: "auto",
              borderRadius: "var(--mantine-radius-sm)",
              display: "block",
            }}
          />
        )}
        {fileType === "audio" && (
          // oxlint-disable-next-line jsx-a11y/media-has-caption -- User-provided media does not include a captions track.
          <audio
            src={decryptedUrl}
            controls
            style={{
              width: "100%",
              minWidth: "300px",
              display: "block",
            }}
          />
        )}
        <AttachmentDetails
          filename={content.filename ?? ""}
          fileSize={fileSize}
          align={align}
        />
      </Box>
    </Paper>
  );
};
