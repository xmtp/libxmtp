import { ConnectModel } from "@/core/connectModel.ts";
import { launchEnvironment } from "@/platform/messengerPlatform";
import { probeBackend, sessionController } from "@/session";

/** `ConnectState.address` default when the launch environment has none. */
const DEFAULT_BACKEND_URL = "http://127.0.0.1:5050";

let shared: ConnectModel | undefined;

/**
 * The app's one Connect model: it lives as long as the app, so the typed
 * address and server name survive Sign out and Delete local data. The screen
 * calls `appear()` on mount and `dispose()` on unmount.
 */
export function connectModel(): ConnectModel {
  shared ??= createConnectModel(
    launchEnvironment().backendUrl ?? DEFAULT_BACKEND_URL,
  );
  return shared;
}

/**
 * The Connect store: the pure model wired to the backend probe and the session
 * controller.
 */
function createConnectModel(address: string): ConnectModel {
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
// (`resetIncomplete`, `restoreFailed`), so failures are not shown here.

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
