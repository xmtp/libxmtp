import * as Clipboard from "expo-clipboard";

/** Copies `text` to the system pasteboard ("Copy inbox ID", "Copy server address"). */
export async function copyText(text: string): Promise<void> {
  await Clipboard.setStringAsync(text);
}
