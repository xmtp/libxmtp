import { useEffect, useState } from "react";
import { Platform, useWindowDimensions } from "react-native";

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

/**
 * React Native's iOS font multiplier per content size and the large title
 * size there, for `@ScaledMetric(relativeTo: .largeTitle)`.
 */
const largeTitleSizes: readonly (readonly [number, number])[] = [
  [0.823, 31],
  [0.882, 32],
  [0.941, 33],
  [1, 34],
  [1.118, 36],
  [1.235, 38],
  [1.353, 40],
  [1.786, 44],
  [2.143, 48],
  [2.643, 52],
  [3.143, 56],
  [3.571, 60],
];

/** The `@ScaledMetric(relativeTo: .largeTitle)` factor: 1 at the default size. */
export function useLargeTitleScale(): number {
  const { fontScale } = useWindowDimensions();
  if (Platform.OS !== "ios") return fontScale;
  let nearest: readonly [number, number] = [1, 34];
  for (const entry of largeTitleSizes) {
    if (Math.abs(entry[0] - fontScale) < Math.abs(nearest[0] - fontScale))
      nearest = entry;
  }
  return nearest[1] / 34;
}
