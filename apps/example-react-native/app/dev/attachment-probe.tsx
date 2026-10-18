// Debug-only attachment settings for checks the shared backend cannot produce
// on demand. `xcrun simctl openurl <udid> "xmtp-messenger-rn:///dev/attachment-probe?action=<a>&n=<new value>"`
// Actions: `limit` (`&bytes=<n>`: the upload limit chats read on their next
// open), `clearLimit`, `holdUploads` (uploads wait, so the app can be stopped
// mid-upload) and `releaseUploads`. The probe then goes back.

import { Stack, router, useLocalSearchParams } from "expo-router";
import { useEffect, useRef } from "react";
import { Text } from "react-native";

import { attachmentDebug } from "@/features/attachments/sdkAttachmentGateway";

export default function AttachmentProbe() {
  const params = useLocalSearchParams<{
    action?: string;
    bytes?: string;
    n?: string;
  }>();
  const ran = useRef<string | undefined>(undefined);

  useEffect(() => {
    if (!__DEV__ || params.action === undefined) return;
    const id = `${params.action}|${params.bytes ?? ""}|${params.n ?? ""}`;
    if (ran.current === id) return;
    ran.current = id;
    switch (params.action) {
      case "limit":
        attachmentDebug.limit = BigInt(params.bytes ?? "0");
        break;
      case "clearLimit":
        attachmentDebug.limit = undefined;
        break;
      case "holdUploads":
        attachmentDebug.holdUploads = true;
        break;
      case "releaseUploads":
        attachmentDebug.holdUploads = false;
        break;
      default:
        console.warn(`[attachment-probe] unknown action ${params.action}`);
    }
    console.log(`[attachment-probe] ${params.action}`);
    if (router.canGoBack()) router.back();
  }, [params.action, params.bytes, params.n]);

  return (
    <>
      <Stack.Screen options={{ title: "Attachment probe" }} />
      <Text>{__DEV__ ? "Attachment probe" : "Debug builds only."}</Text>
    </>
  );
}
