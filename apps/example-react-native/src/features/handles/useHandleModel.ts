import { useEffect, useMemo, useState } from "react";

import type { HandleModel, HandleState } from "@/core/handleModel.ts";
import type { HandleMode } from "@/core/handles.ts";
import { handleCopy } from "@/features/handles/copy";
import { createHandleModel } from "@/features/handles/handleStore";
import type { ActiveSession } from "@/session";
import type { AlertDialogProps, AlertDialogRequest } from "@/ui/AlertDialog";
import { useStore } from "@/ui/useStore";

/**
 * The route's handle model: created once per mount, loaded on appear,
 * disposed on unmount. `confirmation` is the alert that follows
 * `state.confirm` (change mode only); render it with `AlertDialog`.
 */
export function useHandleModel(
  session: ActiveSession,
  mode: HandleMode,
): {
  readonly model: HandleModel;
  readonly state: HandleState;
  readonly confirmation: AlertDialogProps;
} {
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
  const request = useMemo<AlertDialogRequest | undefined>(
    () =>
      confirm === undefined
        ? undefined
        : {
            title: hasCurrent ? handleCopy.changeTitle : handleCopy.claimTitle,
            message: handleCopy.confirmMessage(confirm),
            buttons: [
              {
                text: hasCurrent ? handleCopy.change : handleCopy.claim,
                onPress: () => {
                  model.confirmClaim();
                },
              },
              {
                text: handleCopy.cancel,
                style: "cancel",
                onPress: () => {
                  model.cancelClaim();
                },
              },
            ],
          },
    [confirm, hasCurrent, model],
  );
  return { model, state, confirmation: { request, onDismiss: keepClaim } };
}

/**
 * Both buttons clear `confirm`. The dismissal can arrive before the button's
 * action, so it must not cancel a claim the user just confirmed.
 */
function keepClaim(): void {}
