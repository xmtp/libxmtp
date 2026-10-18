import { ConnectModel } from "@/core/connectModel.ts";
import { probeBackend, sessionController } from "@/session";

/** `ConnectState.address` default when the launch environment has none. */
export const DEFAULT_BACKEND_URL = "http://127.0.0.1:5050";

/**
 * The Connect route store (`ConnectStore.swift`): the pure model wired to the
 * backend probe and the session controller.
 */
export function createConnectModel(address: string): ConnectModel {
  return new ConnectModel(address, {
    probe: (url) => probeBackend(url),
    connect: ({ url, token, name, signal, onReserved }) =>
      sessionController.connect({
        url,
        token,
        ...(name === undefined ? {} : { name }),
        signal,
        onReserved,
      }),
    generation: () => sessionController.fence.currentGeneration(),
  });
}

// The recovery actions report their outcome through the session state
// (`resetIncomplete`, `restoreFailed`), like iOS, so failures are not shown here.

/** Restore failed: "Retry". */
export function retryRestore(): void {
  sessionController.retryRestore().catch(() => {});
}

/** Restore failed: "Delete local data" after the confirmation. */
export function deleteLocalData(): void {
  sessionController.deleteLocalData().catch(() => {});
}

/** Reset incomplete: "Continue local data removal". */
export function continueReset(): void {
  sessionController.continueReset().catch(() => {});
}
