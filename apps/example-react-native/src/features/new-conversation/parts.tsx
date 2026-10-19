import { ActivityIndicator, StyleSheet, View } from "react-native";

import { personInitials } from "@/core/avatar.ts";
import {
  type NewConversationState,
  type NewConversationStore,
  type Person,
  personLabel,
} from "@/core/newConversation.ts";
import { newConversationCopy as copy } from "@/features/new-conversation/copy";
import { RemoveRecipientButton } from "@/features/new-conversation/RemoveRecipientButton";
import { Avatar } from "@/ui/Avatar";
import { useColors } from "@/ui/colors";
import { FormRow, FormSection } from "@/ui/Form";
import { GlassButton } from "@/ui/GlassButton";
import { IdentityText } from "@/ui/Identity";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";

/** Separator start of an avatar row: under the 44 pt avatar's initial. */
const AVATAR_ROW_SEPARATOR = 32.5;

/** The principal title: "New message" or "New group" over "Step N of 2". */
export function StepTitle({
  title,
  step,
}: {
  readonly title: string;
  readonly step: string;
}) {
  return (
    <View
      accessible
      accessibilityRole="header"
      accessibilityLabel={`${title}, ${step}`}
      style={styles.title}>
      <Text variant="headline" numberOfLines={1}>
        {title}
      </Text>
      <Text variant="caption" color="themeSecondaryLabel" numberOfLines={1}>
        {step}
      </Text>
    </View>
  );
}

function PersonAvatar({ person }: { readonly person: Person }) {
  return (
    <Avatar
      seed={person.inboxId}
      label={personInitials(personLabel(person), person.handle === undefined)}
    />
  );
}

/** A "To:" token: the identity on a 10 percent tint capsule with a remove button. */
export function RecipientChip({
  person,
  onRemove,
}: {
  readonly person: Person;
  readonly onRemove: () => void;
}) {
  const palette = useColors();
  const label = personLabel(person);
  return (
    <View style={[styles.chip, { backgroundColor: palette.tintFill10 }]}>
      <View
        accessible
        accessibilityLabel={label}
        testID={`recipient.${person.inboxId}`}>
        <IdentityText inboxId={person.inboxId} handle={person.handle} />
      </View>
      <RemoveRecipientButton
        accessibilityLabel={copy.remove(label)}
        onPress={onRemove}
      />
    </View>
  );
}

/** A "People from your chats" row: avatar, identity and a check circle. */
export function SelectablePersonRow({
  person,
  selected,
  onPress,
}: {
  readonly person: Person;
  readonly selected: boolean;
  readonly onPress: () => void;
  readonly separatorInset?: number;
}) {
  return (
    <FormRow
      onPress={onPress}
      accessibilityLabel={personLabel(person).replace(/^@/u, "")}
      accessibilityState={{ selected }}
      style={styles.personRow}>
      <PersonAvatar person={person} />
      <IdentityText
        inboxId={person.inboxId}
        handle={person.handle}
        numberOfLines={1}
        style={styles.flex}
      />
      <Symbol
        name={selected ? "checkmark.circle.fill" : "circle"}
        color="tint"
      />
    </FormRow>
  );
}
SelectablePersonRow.separatorInset = (): number => AVATAR_ROW_SEPARATOR;

/** A review member row: avatar and identity (or "You"). */
export function MemberRow({
  person,
}: {
  readonly person?: Person;
  readonly separatorInset?: number;
}) {
  return (
    <FormRow style={styles.personRow}>
      {person === undefined ? (
        <>
          <Avatar seed="own" label="Y" />
          <Text style={styles.flex}>{copy.you}</Text>
        </>
      ) : (
        <>
          <PersonAvatar person={person} />
          <IdentityText
            inboxId={person.inboxId}
            handle={person.handle}
            numberOfLines={1}
            style={styles.flex}
          />
        </>
      )}
    </FormRow>
  );
}
MemberRow.separatorInset = (): number => AVATAR_ROW_SEPARATOR;

/**
 * The progress row while loading or busy, the bootstrap error with Retry,
 * and (in the review) the action error.
 */
export function LoadingAndError({
  state,
  store,
  showError,
}: {
  readonly state: NewConversationState;
  readonly store: NewConversationStore;
  readonly showError: boolean;
}) {
  return (
    <>
      {state.busy || state.loadingPeople ? (
        <FormSection>
          <FormRow style={styles.center}>
            <ActivityIndicator />
          </FormRow>
        </FormSection>
      ) : null}
      {state.bootstrapError === undefined ? null : (
        <FormSection>
          <FormRow style={styles.bootstrapError}>
            <Text color="systemRed">{state.bootstrapError}</Text>
            <GlassButton
              title={copy.retry}
              testID="newConversation.retryBootstrap"
              disabled={state.busy || state.loadingPeople}
              onPress={() => {
                store.load();
              }}
            />
          </FormRow>
        </FormSection>
      )}
      {showError && state.error !== undefined ? (
        <FormSection>
          <FormRow>
            <Text color="systemRed" style={styles.flex}>
              {state.error}
            </Text>
          </FormRow>
        </FormSection>
      ) : null}
    </>
  );
}

const styles = StyleSheet.create({
  title: { alignItems: "center", gap: 2 },
  chip: {
    alignItems: "center",
    borderCurve: "continuous",
    borderRadius: 999,
    flexDirection: "row",
    gap: 5,
    paddingHorizontal: 10,
    paddingVertical: 6,
  },
  personRow: { paddingVertical: 15 },
  flex: { flex: 1 },
  center: { justifyContent: "center" },
  bootstrapError: { alignItems: "flex-start", flexDirection: "column", gap: 8 },
});
