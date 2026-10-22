// Debug-only attachment settings for checks the shared backend cannot produce
// on demand. `xcrun simctl openurl <udid> "xmtp-messenger-rn:///dev/attachment-probe?action=<a>&n=<new value>"`
// Actions: `limit` (`&bytes=<n>`: the upload limit chats read on their next
// open), `clearLimit`, `holdUploads` (uploads wait, so the app can be stopped
// mid-upload) and `releaseUploads`. The probe then goes back.

import { router, useLocalSearchParams } from "expo-router";
import { useEffect, useRef } from "react";

import { decimalToBigint } from "@/core/bigint.ts";
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
    // Any app can open this URL: log fixed text or a known action name, never
    // a value taken from the link.
    let known = true;
    switch (params.action) {
      case "limit": {
        // A count that is not decimal digits leaves the limit as it was; the
        // probe still goes back.
        const bytes = decimalToBigint(params.bytes ?? "0");
        if (bytes === undefined || bytes < 0n) {
          console.warn("[attachment-probe] invalid bytes");
        } else {
          attachmentDebug.limit = bytes;
        }
        break;
      }
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
        known = false;
        console.warn("[attachment-probe] unknown action");
    }
    if (known) console.log(`[attachment-probe] ${params.action}`);
    if (router.canGoBack()) router.back();
  }, [params.action, params.bytes, params.n]);

  // The root layout registers this route for Debug builds only, with its
  // title. The probe goes back as soon as its action ran.
  return null;
}
