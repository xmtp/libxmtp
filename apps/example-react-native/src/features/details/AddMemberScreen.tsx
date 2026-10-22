import { Stack, router } from "expo-router";
import { useEffect, useRef, useState } from "react";
import { ActivityIndicator, StyleSheet } from "react-native";

import { detailsCopy } from "@/features/details/copy";
import {
  DetailsErrorSection,
  MemberIdentityRow,
} from "@/features/details/DetailsRows";
import { useSharedDetailsStore } from "@/features/details/detailsStores";
import { useSheetFormProps } from "@/features/details/sheetDetent";
import { type ActiveSession, useActiveSession } from "@/session";
import { FormRow, FormScreen, FormSection } from "@/ui/Form";
import { TextFieldRow } from "@/ui/FormRows";
import { Text } from "@/ui/Text";
import { ToolbarTextButton } from "@/ui/ToolbarTextButton";
import { useStore } from "@/ui/useStore";

export interface AddMemberScreenProps {
  readonly conversationId: string;
}

/**
 * The Add member sheet: a recipient field with a 300 ms lookup and a "Found"
 * row, Cancel and Add. Add resolves the input again and adds the inbox; the
 * sheet closes when that finishes without an error.
 */
export function AddMemberScreen({ conversationId }: AddMemberScreenProps) {
  const session = useActiveSession();
  if (session === undefined) return <FormScreen />;
  return (
    <AddMemberForm
      key={session.generation}
      session={session}
      conversationId={conversationId}
    />
  );
}

function AddMemberForm({
  session,
  conversationId,
}: {
  readonly session: ActiveSession;
  readonly conversationId: string;
}) {
  const form = useSheetFormProps();
  const store = useSharedDetailsStore(session, conversationId);
  const state = useStore(store);
  const [input, setInput] = useState("");
  const submitted = useRef(false);
  const wasBusy = useRef(state.busy);
  const { busy, error } = state;

  // The shared Details store keeps the last lookup. Each opening starts with
  // an empty field and no preview.
  useEffect(() => {
    store.resolvePreview("");
  }, [store]);

  // Close only when an Add from this sheet finishes without an error.
  // Typing clears the error but never closes.
  useEffect(() => {
    const finished = wasBusy.current && !busy;
    wasBusy.current = busy;
    if (finished && submitted.current && error === undefined) {
      submitted.current = false;
      router.back();
    }
  }, [busy, error]);

  return (
    <FormScreen {...form} contentContainerStyle={styles.content}>
      <Stack.Screen options={{ title: detailsCopy.addMember }} />
      <ToolbarTextButton
        title={detailsCopy.cancel}
        placement="left"
        onPress={() => {
          router.back();
        }}
      />
      <ToolbarTextButton
        title={detailsCopy.add}
        placement="right"
        prominent
        confirmation
        disabled={state.recipient === undefined || state.busy}
        onPress={() => {
          submitted.current = true;
          store.send({ kind: "mutate", mutation: { kind: "add", input } });
        }}
      />
      <FormSection footer={detailsCopy.addMemberFooter}>
        <TextFieldRow
          testID="member.recipient"
          accessibilityLabel={detailsCopy.recipientPlaceholder}
          placeholder={detailsCopy.recipientPlaceholder}
          value={input}
          onChangeText={(value) => {
            setInput(value);
            store.resolvePreview(value);
          }}
          autoCapitalize="none"
          autoCorrect={false}
          autoFocus={false}
        />
        {state.recipient === undefined ? null : (
          <MemberIdentityRow
            member={state.recipient}
            trailing={
              <Text
                variant="caption"
                color="secondaryLabel"
                style={styles.found}>
                {detailsCopy.found}
              </Text>
            }
          />
        )}
      </FormSection>
      <DetailsErrorSection error={state.error ?? state.recipientError} />
      {state.busy ? (
        <FormSection>
          <FormRow style={styles.center}>
            <ActivityIndicator />
          </FormRow>
        </FormSection>
      ) : null}
      {state.pendingMembers.size > 0 ? (
        <FormSection>
          {[...state.pendingMembers.values()].map((member) => (
            <MemberIdentityRow key={member.id} member={member} />
          ))}
        </FormSection>
      ) : null}
    </FormScreen>
  );
}

const styles = StyleSheet.create({
  // A form in a sheet starts its first section 35 pt below the bar.
  content: { paddingTop: 35 },
  center: { justifyContent: "center" },
  // The row spaces its items 12 pt apart, not the default 8.
  found: { marginLeft: -4 },
});
