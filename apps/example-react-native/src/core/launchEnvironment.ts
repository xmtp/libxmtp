/** Launch-time settings, from `MessengerPlatform.launchEnvironment()`. */
export interface LaunchEnvironment {
  /** `XMTP_BACKEND_URL`, when the launch set it. It can be empty. */
  readonly backendUrl: string | undefined;
  /** True only when `XMTP_ALLOW_PRIVATE_ATTACHMENTS` is exactly `"1"`. */
  readonly allowPrivateAttachments: boolean;
  /** `XMTP_UI_TEST_ATTACHMENT` (`name:base64`) for the Debug test file. */
  readonly uiTestAttachment: string | undefined;
}

/** Reads the launch variables. Values that are not strings are ignored. */
export function parseLaunchEnvironment(
  raw: Readonly<Record<string, unknown>>,
): LaunchEnvironment {
  const text = (key: string): string | undefined => {
    const value = raw[key];
    return typeof value === "string" ? value : undefined;
  };
  return {
    backendUrl: text("XMTP_BACKEND_URL"),
    allowPrivateAttachments: text("XMTP_ALLOW_PRIVATE_ATTACHMENTS") === "1",
    uiTestAttachment: text("XMTP_UI_TEST_ATTACHMENT"),
  };
}
