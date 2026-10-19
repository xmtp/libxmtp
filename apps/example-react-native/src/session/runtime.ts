// The app's one session controller, wired to the device and the SDK, and the
// app lifecycle: restore on launch and on foreground.

import { AppState } from "react-native";

import {
  SessionController,
  type SessionState,
} from "@/core/sessionController.ts";
import { randomUuid } from "@/platform/random";
import { ActiveSession } from "@/session/activeSession";
import { DeviceProfileFiles, DeviceSecretStore } from "@/session/deviceStorage";
import { type OpenedClient, SdkClientFactory } from "@/session/sdkClients";

export type AppSessionState = SessionState<ActiveSession>;

type AppSessionController = SessionController<OpenedClient, ActiveSession>;

/** Foreground means `active`. `inactive` (app switcher) and `background` are not. */
function isForeground(state: string | null | undefined): boolean {
  return state !== "background" && state !== "inactive";
}

interface Runtime {
  readonly controller: AppSessionController;
  foreground: boolean;
  lifecycle?: { remove(): void };
}

function createRuntime(): Runtime {
  const runtime: Runtime = {
    foreground: isForeground(AppState.currentState),
    controller: new SessionController<OpenedClient, ActiveSession>({
      secrets: new DeviceSecretStore(),
      files: new DeviceProfileFiles(),
      factory: new SdkClientFactory(),
      newProfileId: randomUuid,
      bind: (request) => {
        const session = new ActiveSession(request, {
          foreground: runtime.foreground,
        });
        session.streams.start();
        return session;
      },
    }),
  };
  return runtime;
}

// Fast Refresh re-evaluates this module when a session file changes. A second
// controller would open a second client on the same database, whose default
// message reader fails with ConsumerOwned, so the runtime lives on globalThis
// for the life of the JS runtime. Reload the app to pick up session changes.
const shared = globalThis as typeof globalThis & {
  __xmtpExampleSessionRuntime?: Runtime;
};
const runtime = (shared.__xmtpExampleSessionRuntime ??= createRuntime());

/**
 * The session controller. Screens read its state through `useSessionState`
 * and call its actions: `start`, `restore`, `connect`, `retryRestore`,
 * `signOut`, `deleteLocalData`, `continueReset`, `setAppearance`.
 */
export const sessionController: AppSessionController = runtime.controller;

/** The open session, if any. */
export function currentSession(): ActiveSession | undefined {
  const state = sessionController.getState();
  return state.kind === "active" ? state.session : undefined;
}

function foregroundChanged(active: boolean): void {
  if (active === runtime.foreground) return;
  runtime.foreground = active;
  const session = currentSession();
  void session?.streams.setForeground(active).then(() => {
    if (__DEV__) {
      console.info(
        `[session] ${active ? "foreground" : "background"}: message reader ${session.streams.readingMessages ? "running" : "paused"}`,
      );
    }
  });
  if (active) void sessionController.restore();
}

/**
 * Starts the app session once: storage, appearance and restore, then follows
 * the app lifecycle. Foreground restores (when signed in but not open) and
 * runs the default message reader; inactive and background close that reader
 * so its consumer lease is released. An app that the OS suspended or killed
 * would otherwise hold it, and the next reader would fail with `ConsumerOwned`.
 */
export function startSession(): Promise<void> {
  runtime.lifecycle ??= AppState.addEventListener("change", (next) => {
    foregroundChanged(isForeground(next));
  });
  return sessionController.start();
}
