import type { RemoteAttachment } from "@xmtp/react-native-sdk";

import { type ChatMessage, messageDate } from "@/core/chatMessages.ts";
import { timeOfDay } from "@/core/dates.ts";
import { AttachmentCard } from "@/features/attachments/AttachmentCard";
import { useChatAttachments } from "@/features/attachments/chatAttachments";
import { attachmentPresentationCopy } from "@/features/attachments/copy";
import { useChatContext } from "@/features/chat/ChatContext";
import { useStore } from "@/ui/useStore";

export interface AttachmentContentProps {
  readonly row: ChatMessage;
  readonly file: RemoteAttachment;
  readonly isOutgoing: boolean;
  /** Downloads need an allowed conversation ("Closed until you accept"). */
  readonly allowed: boolean;
}

/** A file message inside its bubble, using the open chat's attachment store. */
export function ChatAttachmentCard({
  row,
  file,
  isOutgoing,
  allowed,
}: AttachmentContentProps) {
  const { runtime } = useChatContext();
  const store = useChatAttachments(runtime);
  const identities = useStore(runtime.session.identities);
  const sender = isOutgoing
    ? attachmentPresentationCopy.you
    : (identities.get(row.sender)?.handle ?? row.sender.slice(0, 12));
  return (
    <AttachmentCard
      store={store}
      messageId={row.id}
      file={file}
      allowed={allowed}
      appearance={isOutgoing ? "outgoing" : "incoming"}
      metadata={`${sender} · ${timeOfDay(messageDate(row))}`}
    />
  );
}
