import { Stack, router } from "expo-router";
import { useEffect, useRef } from "react";
import { StyleSheet, TextInput, View } from "react-native";

import {
  type NewConversationStore,
  addDisabled,
  canContinue,
} from "@/core/newConversation.ts";
import { newConversationCopy as copy } from "@/features/new-conversation/copy";
import { useNewConversation } from "@/features/new-conversation/NewConversationSheet";
import {
  LoadingAndError,
  RecipientChip,
  SelectablePersonRow,
  StepTitle,
} from "@/features/new-conversation/parts";
import { useScrollEdge } from "@/features/new-conversation/scrollEdge";
import { useColors } from "@/ui/colors";
import { FormRow, FormScreen, FormSection } from "@/ui/Form";
import { singleLineBreakMode } from "@/ui/FormRows";
import { GlassButton } from "@/ui/GlassButton";
import { SegmentedPicker } from "@/ui/SegmentedPicker";
import { Text } from "@/ui/Text";
import { useToolbarIcon } from "@/ui/toolbarIcon";
import { ToolbarTextButton } from "@/ui/ToolbarTextButton";
import { fontOf } from "@/ui/typography";
import { useStore } from "@/ui/useStore";

type Kind = "dm" | "group";

const kinds = [
  { value: "dm", label: copy.directMessage },
  { value: "group", label: copy.group },
] as const;

/** New message, step 1: the type, the recipients and people from your chats. */
export function NewConversationScreen() {
  const sheet = useNewConversation();
  if (sheet === undefined) return null;
  return (
    <ChoosePeople
      store={sheet.store}
      close={sheet.close}
      closing={sheet.closing}
    />
  );
}

function ChoosePeople({
  store,
  close,
  closing,
}: {
  readonly store: NewConversationStore;
  readonly close: () => void;
  readonly closing: boolean;
}) {
  const palette = useColors();
  const state = useStore(store);
  const closeIcon = useToolbarIcon("xmark");
  const edge = useScrollEdge();

  // Next shows the review: push it inside the sheet's stack.
  const reviewShown = useRef(false);
  useEffect(() => {
    if (state.reviewing && !reviewShown.current) {
      reviewShown.current = true;
      router.push("/new-conversation/review");
    } else if (!state.reviewing) {
      reviewShown.current = false;
    }
  }, [state.reviewing]);

  const selected = new Set(state.recipients.map((person) => person.inboxId));

  return (
    // SwiftUI Forms inset their content above the keyboard.
    <FormScreen
      avoidsKeyboard
      scrollEventThrottle={16}
      onScroll={edge.onScroll}>
      <Stack.Screen
        options={{
          ...edge.options,
          title: copy.newMessage,
          headerTitle: state.group
            ? () => <StepTitle title={copy.newMessage} step={copy.step1} />
            : undefined,
        }}
      />
      <Stack.Toolbar placement="left">
        <Stack.Toolbar.Button
          icon={closeIcon}
          tintColor={palette.tint}
          accessibilityLabel={copy.cancel}
          disabled={state.creating}
          onPress={close}
        />
      </Stack.Toolbar>
      <ToolbarTextButton
        title={state.group ? copy.next : copy.create}
        placement="right"
        prominent
        disabled={closing || !canContinue(state)}
        onPress={() => {
          store.next();
        }}
      />
      <View style={styles.picker}>
        <SegmentedPicker<Kind>
          options={kinds}
          disabled={state.creating}
          selection={state.group ? "group" : "dm"}
          accessibilityLabel={copy.conversationType}
          onSelectionChange={(value) => {
            store.setGroup(value === "group");
          }}
        />
      </View>
      <FormSection
        style={styles.recipients}
        footer={
          <View style={styles.footer}>
            {state.error === undefined ? null : (
              <Text variant="footnote" color="systemRed">
                {state.error}
              </Text>
            )}
            <Text variant="footnote" color="secondaryLabel">
              {state.group ? copy.groupHint : copy.dmHint}
            </Text>
            {state.group && state.maximumMembers > 0 ? (
              <Text variant="footnote" color="secondaryLabel">
                {copy.limitHint(state.maximumMembers)}
              </Text>
            ) : null}
          </View>
        }>
        <FormRow style={styles.flow}>
          <Text color="themeSecondaryLabel">{copy.to}</Text>
          {state.recipients.map((person) => (
            <RecipientChip
              key={person.inboxId}
              person={person}
              onRemove={() => {
                store.remove(person.inboxId);
              }}
            />
          ))}
        </FormRow>
        <FormRow>
          <TextInput
            editable={!state.creating}
            value={state.input}
            onChangeText={(text) => {
              store.setInput(text);
            }}
            onSubmitEditing={() => {
              store.add();
            }}
            placeholder={copy.inputPlaceholder}
            placeholderTextColor={palette.placeholderText}
            selectionColor={palette.tint}
            autoCapitalize="none"
            autoCorrect={false}
            spellCheck={false}
            numberOfLines={1}
            lineBreakModeIOS={singleLineBreakMode}
            style={[fontOf("body"), styles.input, { color: palette.label }]}
          />
          <GlassButton
            title={copy.add}
            disabled={addDisabled(state)}
            onPress={() => {
              store.add();
            }}
          />
        </FormRow>
      </FormSection>
      {state.people.length === 0 ? null : (
        <FormSection
          style={styles.people}
          header={copy.peopleHeader}
          headerTrailing={
            <Text variant="caption" color="secondaryLabel">
              {copy.peopleSaved}
            </Text>
          }>
          {state.people.map((person) => (
            <SelectablePersonRow
              key={person.inboxId}
              person={person}
              selected={selected.has(person.inboxId)}
              onPress={() => {
                store.toggle(person);
              }}
            />
          ))}
        </FormSection>
      )}
      <LoadingAndError state={state} store={store} showError={false} />
    </FormScreen>
  );
}

const styles = StyleSheet.create({
  // The picker sits in a full-width 52 pt list row with no row insets.
  picker: { justifyContent: "center", marginHorizontal: 16, minHeight: 52 },
  // Measured on iOS 27: 15.5 pt from the picker row, 31.9 pt from the footer.
  recipients: { paddingTop: 15.5 },
  people: { paddingTop: 31.9 },
  flow: { alignItems: "flex-start", flexWrap: "wrap", gap: 8 },
  footer: { gap: 6 },
  input: { flex: 1, paddingVertical: 0 },
});
