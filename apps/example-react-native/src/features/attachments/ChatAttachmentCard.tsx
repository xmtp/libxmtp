import type { RemoteAttachment } from "@xmtp/react-native-sdk";

import type { ChatMessage } from "@/core/chatMessages.ts";
import { AttachmentCard } from "@/features/attachments/AttachmentCard";
import { useChatAttachments } from "@/features/attachments/chatAttachments";
import { useChatContext } from "@/features/chat/ChatContext";

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
  return (
    <AttachmentCard
      store={store}
      messageId={row.id}
      file={file}
      allowed={allowed}
      appearance={isOutgoing ? "outgoing" : "incoming"}
    />
  );
}
