import { useEffect, useState } from "react";

import type { HandleModel, HandleState } from "@/core/handleModel.ts";
import type { HandleMode } from "@/core/handles.ts";
import { handleCopy } from "@/features/handles/copy";
import { createHandleModel } from "@/features/handles/handleStore";
import type { ActiveSession } from "@/session";
import { showAlert } from "@/ui/dialogs";
import { useStore } from "@/ui/useStore";

/**
 * The route's handle model: created once per mount, loaded on appear,
 * disposed on unmount. The confirmation alert follows `state.confirm`.
 */
export function useHandleModel(
  session: ActiveSession,
  mode: HandleMode,
): { readonly model: HandleModel; readonly state: HandleState } {
  const [model] = useState(() => createHandleModel(session, mode));
  useEffect(() => {
    void model.load();
    return () => {
      model.dispose();
    };
  }, [model]);
  const state = useStore(model);
  const confirm = state.confirm;
  const hasCurrent = state.current !== undefined;
  useEffect(() => {
    if (confirm === undefined) return;
    showAlert(
      hasCurrent ? handleCopy.changeTitle : handleCopy.claimTitle,
      handleCopy.confirmMessage(confirm),
      [
        {
          text: handleCopy.cancel,
          style: "cancel",
          onPress: () => {
            model.cancelClaim();
          },
        },
        {
          text: hasCurrent ? handleCopy.change : handleCopy.claim,
          onPress: () => {
            model.confirmClaim();
          },
        },
      ],
    );
  }, [confirm, hasCurrent, model]);
  return { model, state };
}
