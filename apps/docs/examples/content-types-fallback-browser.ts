// #region example
import type { Message } from "@xmtp/browser-sdk";

export function displayText(message: Message): string | undefined {
  switch (message.content.kind) {
    case "text":
      return message.content.value;
    case "readReceipt":
      return undefined;
    default:
      return message.fallback;
  }
}
// #endregion example
