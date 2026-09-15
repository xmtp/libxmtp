import {
  NotificationFailure,
  NotificationOverride as BindingNotificationOverride,
  NotificationStateKind,
  type ConsentState,
  type NotificationConfig as BindingNotificationConfig,
  type NotificationState as BindingNotificationState,
} from "@xmtp/node-bindings";

/** Delivery channel for this installation. */
export type NotificationChannel =
  | { type: "apns"; token: string }
  | { type: "fcm"; token: string }
  | { type: "http"; url: string; signingKey: Uint8Array };

/** Notification delivery and conversation rules. */
export type NotificationConfig = {
  channel: NotificationChannel;
  /** Defaults to Allowed. An empty list selects no consent states. */
  consentStates?: ConsentState[];
  /** Defaults to true. */
  includeWelcomes?: boolean;
  /** Defaults to false. */
  includeSyncGroups?: boolean;
  /** Defaults to false. */
  includeCommits?: boolean;
};

/** Reset to the configured consent rules with "default". */
export type NotificationOverride = "enabled" | "disabled" | "default";

/** A notification failure with a stable error code. */
export class NotificationError extends Error {
  constructor(
    public readonly code: string,
    options?: ErrorOptions,
  ) {
    super(code, options);
    this.name = "NotificationError";
  }
}

/** Local notification state. Reading it makes no backend request. */
export type NotificationState =
  | { state: "disabled" }
  | { state: "enabled" }
  | { state: "failed"; error: NotificationError };

export const toBindingNotificationConfig = (
  config: NotificationConfig,
): BindingNotificationConfig => {
  const { channel, ...rules } = config;
  return {
    ...rules,
    channel: channel.type,
    ...(channel.type === "http"
      ? { url: channel.url, signingKey: Array.from(channel.signingKey) }
      : { token: channel.token }),
  };
};

export const toNotificationState = (
  value: BindingNotificationState,
): NotificationState => {
  switch (value.state) {
    case NotificationStateKind.Disabled:
      return { state: "disabled" };
    case NotificationStateKind.Enabled:
      return { state: "enabled" };
    case NotificationStateKind.Failed: {
      const codes: Record<NotificationFailure, string> = {
        [NotificationFailure.PermissionDenied]: "PermissionDenied",
        [NotificationFailure.InvalidArgument]: "InvalidArgument",
        [NotificationFailure.OutOfRange]: "OutOfRange",
        [NotificationFailure.Unimplemented]: "Unimplemented",
        [NotificationFailure.ChannelNotConfigured]: "ChannelNotConfigured",
      };
      if (value.failure == null) {
        throw new Error("Notification failure is missing its error");
      }
      return {
        state: "failed",
        error: new NotificationError(
          `NotificationError::${codes[value.failure]}`,
        ),
      };
    }
  }
};

export const toBindingNotificationOverride = (
  value: NotificationOverride,
): BindingNotificationOverride => {
  switch (value) {
    case "enabled":
      return BindingNotificationOverride.Enabled;
    case "disabled":
      return BindingNotificationOverride.Disabled;
    case "default":
      return BindingNotificationOverride.Default;
  }
};

export const throwNotificationError = (error: unknown): never => {
  if (error instanceof Error) {
    const code = /^\[(NotificationError::[^\]]+)\]/.exec(error.message)?.[1];
    if (code) throw new NotificationError(code, { cause: error });
  }
  throw error;
};
