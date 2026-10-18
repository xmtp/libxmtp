import { XmtpProvider } from "@xmtp/react-native-sdk/react";
import { type ReactNode, useEffect } from "react";

import type { Appearance } from "@/core/sessionIndex.ts";
import type { ActiveSession } from "@/session/activeSession";
import {
  type AppSessionState,
  sessionController,
  startSession,
} from "@/session/runtime";
import { useStore } from "@/session/useStore";

/**
 * Starts the session (storage, restore, app lifecycle) and provides the open
 * client to the SDK's React helpers. The client is borrowed: the session owns
 * it and ends it; `XmtpProvider` never does.
 */
export function SessionProvider({
  children,
}: {
  readonly children: ReactNode;
}) {
  const state = useSessionState();
  useEffect(() => {
    // Idempotent: a StrictMode replay or a remount joins the same start.
    void startSession();
  }, []);
  const client = state.kind === "active" ? state.session.client : null;
  return <XmtpProvider client={client}>{children}</XmtpProvider>;
}

/** The session state: starting, storageFailed, signedOut, opening, active, restoreFailed or resetIncomplete. */
export function useSessionState(): AppSessionState {
  return useStore(sessionController);
}

/** The open session, or undefined while none is open. */
export function useActiveSession(): ActiveSession | undefined {
  const state = useSessionState();
  return state.kind === "active" ? state.session : undefined;
}

/** The global appearance setting ("system", "light" or "dark"). */
export function useAppearance(): Appearance {
  return useStore(sessionController.appearance);
}
