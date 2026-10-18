// The session API for screens and stores.

export { ActiveSession, type OwnHandles } from "@/session/activeSession";
export {
  SessionProvider,
  useActiveSession,
  useAppearance,
  useSessionState,
} from "@/session/SessionProvider";
export {
  type AppSessionState,
  currentSession,
  sessionController,
  startSession,
} from "@/session/runtime";
export {
  type BackendProbeResult,
  type OpenedClient,
  connectionError,
  handleError,
  isIdentityNotFound,
  probeBackend,
} from "@/session/sdkClients";
export { useStore } from "@/session/useStore";
