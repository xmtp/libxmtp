import { Stack } from "expo-router";
import { useEffect, useState } from "react";
import { StyleSheet } from "react-native";

import type { DetailsStore } from "@/core/details.ts";
import {
  DISAPPEARING_ERROR,
  DISAPPEARING_PRESETS,
  DisappearingDurationError,
  disappearingNanoseconds,
  isPresetDuration,
  secondsText,
} from "@/core/disappearing.ts";
import { DetailsErrorSection } from "@/features/details/DetailsRows";
import { useOwnDetailsStore } from "@/features/details/detailsStores";
import { disappearingCopy } from "@/features/disappearing/copy";
import { type ActiveSession, useActiveSession } from "@/session";
import { FormRow, FormScreen, FormSection } from "@/ui/Form";
import { RadioRow, TextFieldRow } from "@/ui/FormRows";
import { GlassButton } from "@/ui/GlassButton";
import { useStore } from "@/ui/useStore";

export interface DisappearingScreenProps {
  readonly conversationId: string;
}

/**
 * Disappearing messages (`DisappearingFeatureView`): presets that save at
 * once, and Custom with a seconds field and "Save duration". 0 turns it off;
 * a duration whose nanoseconds overflow Int64 is rejected before any write.
 */
export function DisappearingScreen({
  conversationId,
}: DisappearingScreenProps) {
  const session = useActiveSession();
  if (session === undefined) {
    return (
      <FormScreen>
        <Stack.Screen options={{ title: disappearingCopy.title }} />
      </FormScreen>
    );
  }
  return (
    <DisappearingBody
      key={session.generation}
      session={session}
      conversationId={conversationId}
    />
  );
}

/** Saves `seconds`, or returns the validation error without a write. */
export function saveDuration(
  store: DetailsStore,
  seconds: string,
): string | undefined {
  let retentionNs: bigint | undefined;
  try {
    retentionNs = disappearingNanoseconds(seconds);
  } catch (error) {
    if (error instanceof DisappearingDurationError) return DISAPPEARING_ERROR;
    throw error;
  }
  store.send({
    kind: "mutate",
    mutation:
      retentionNs === undefined
        ? { kind: "disappearing" }
        : { kind: "disappearing", retentionNs },
  });
  return undefined;
}

function DisappearingBody({
  session,
  conversationId,
}: {
  readonly session: ActiveSession;
  readonly conversationId: string;
}) {
  const store = useOwnDetailsStore(session, conversationId);
  const state = useStore(store);
  const [seconds, setSeconds] = useState("0");
  const [custom, setCustom] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);

  // A loaded retention selects its preset, or Custom.
  const retentionNs = state.details?.retentionNs;
  useEffect(() => {
    if (retentionNs === undefined) return;
    const text = secondsText(retentionNs);
    setSeconds(text);
    setCustom(!isPresetDuration(text));
  }, [retentionNs]);

  const save = (value: string) => {
    setError(saveDuration(store, value));
  };

  return (
    <FormScreen automaticallyAdjustKeyboardInsets>
      <Stack.Screen options={{ title: disappearingCopy.title }} />
      <FormSection
        header={disappearingCopy.messageDuration}
        footer={disappearingCopy.durationFooter}>
        {DISAPPEARING_PRESETS.map((preset) => (
          <RadioRow
            key={preset.seconds}
            indicator="checkmark"
            title={preset.label}
            selectedLabel={disappearingCopy.selected}
            selected={seconds === preset.seconds && !custom}
            onPress={() => {
              setCustom(false);
              setSeconds(preset.seconds);
              save(preset.seconds);
            }}
          />
        ))}
        <RadioRow
          indicator="checkmark"
          title={disappearingCopy.custom}
          selectedLabel={disappearingCopy.selected}
          selected={custom}
          onPress={() => {
            setCustom(true);
          }}
        />
      </FormSection>
      {custom ? (
        <FormSection
          header={disappearingCopy.customDuration}
          footer={disappearingCopy.customFooter}>
          <TextFieldRow
            accessibilityLabel={disappearingCopy.durationPlaceholder}
            placeholder={disappearingCopy.durationPlaceholder}
            value={seconds}
            onChangeText={setSeconds}
            keyboardType="number-pad"
          />
          <FormRow style={styles.buttonRow}>
            <GlassButton
              variant="glassProminent"
              title={disappearingCopy.saveDuration}
              onPress={() => {
                save(seconds);
              }}
            />
          </FormRow>
        </FormSection>
      ) : null}
      <DetailsErrorSection error={error ?? state.error} />
    </FormScreen>
  );
}

const styles = StyleSheet.create({
  buttonRow: { paddingVertical: 11 },
});
