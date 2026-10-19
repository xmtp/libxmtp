import { router } from "expo-router";

import {
  type HandleAccount,
  HandleFailure,
  HandleModel,
} from "@/core/handleModel.ts";
import type { HandleMode } from "@/core/handles.ts";
import { type ActiveSession, handleError } from "@/session";

/** Maps an SDK failure to a `HandleFailure` the model understands. */
async function mapped<T>(work: () => Promise<T>): Promise<T> {
  try {
    return await work();
  } catch (error) {
    throw new HandleFailure(handleError(error));
  }
}

/** The account port for one session. */
function sessionAccount(session: ActiveSession): HandleAccount {
  return {
    minimum: session.handleMinimum(),
    claimTimes: () => {
      try {
        return session.preferences.claimTimes();
      } catch {
        return new Map();
      }
    },
    handles: () => session.refreshOwnHandles(),
    availableHandles: (name) =>
      mapped(() => session.client.availableHandles(name)),
    claimHandle: (handle) => mapped(() => session.client.claimHandle(handle)),
    recordClaim: (handle, at) => {
      session.preferences.recordClaim(handle, at);
    },
  };
}

/**
 * The handle route store. Choose handle finishes by replacing the root with
 * Chats; Change handle goes back to Settings. Both then refresh the own handles
 * that Settings shows.
 */
export function createHandleModel(
  session: ActiveSession,
  mode: HandleMode,
): HandleModel {
  const generation = session.generation;
  return new HandleModel({
    account: sessionAccount(session),
    mode,
    isSessionCurrent: () => session.isCurrent(generation),
    finish: () => {
      if (mode === "choose") router.replace("/chats");
      else if (router.canGoBack()) router.back();
      session.refreshOwnHandles().catch(() => {});
    },
    persistSkip: () => {
      session.preferences.skipHandle();
    },
  });
}
