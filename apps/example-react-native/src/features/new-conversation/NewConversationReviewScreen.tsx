import { Stack, router } from "expo-router";
import { useEffect } from "react";
import { StyleSheet, TextInput, View } from "react-native";

import { groupInitials } from "@/core/avatar.ts";
import {
  type NewConversationStore,
  canContinue,
  memberCount,
} from "@/core/newConversation.ts";
import { newConversationCopy as copy } from "@/features/new-conversation/copy";
import { useNewConversation } from "@/features/new-conversation/NewConversationSheet";
import {
  LoadingAndError,
  MemberRow,
  StepTitle,
} from "@/features/new-conversation/parts";
import { useScrollEdge } from "@/features/new-conversation/scrollEdge";
import { Avatar } from "@/ui/Avatar";
import { useColors } from "@/ui/colors";
import { FormRow, FormScreen, FormSection } from "@/ui/Form";
import { TextFieldRow } from "@/ui/FormRows";
import { GlassButton } from "@/ui/GlassButton";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { avatarSizes } from "@/ui/tokens";
import { useToolbarIcon } from "@/ui/toolbarIcon";
import { ToolbarTextButton } from "@/ui/ToolbarTextButton";
import { fontOf } from "@/ui/typography";
import { useStore } from "@/ui/useStore";

/** New message, step 2: review the people and, for a group, its details. */
export function NewConversationReviewScreen() {
  const sheet = useNewConversation();
  if (sheet === undefined) return null;
  return <Review store={sheet.store} closing={sheet.closing} />;
}

function Review({
  store,
  closing,
}: {
  readonly store: NewConversationStore;
  readonly closing: boolean;
}) {
  const palette = useColors();
  const state = useStore(store);
  const backIcon = useToolbarIcon("chevron.left");
  const edge = useScrollEdge();

  // Leaving this screen (Back, Change people, a swipe) leaves the review.
  useEffect(
    () => () => {
      store.back();
    },
    [store],
  );

  const title = state.group ? copy.newGroup : copy.newMessage;
  return (
    <FormScreen
      automaticallyAdjustKeyboardInsets
      scrollEventThrottle={16}
      onScroll={edge.onScroll}
      contentContainerStyle={styles.content}>
      <Stack.Screen
        options={{
          ...edge.options,
          title,
          headerBackVisible: false,
          // The hidden back button also turns off the interactive pop on iOS.
          gestureEnabled: false,
          headerTitle: () => <StepTitle title={title} step={copy.step2} />,
        }}
      />
      <Stack.Toolbar placement="left">
        <Stack.Toolbar.Button
          icon={backIcon}
          tintColor={palette.tint}
          accessibilityLabel={copy.back}
          onPress={() => {
            router.back();
          }}
        />
      </Stack.Toolbar>
      <ToolbarTextButton
        title={copy.create}
        placement="right"
        prominent
        disabled={closing || !canContinue(state)}
        onPress={() => {
          store.create();
        }}
      />
      {state.group ? (
        <View style={styles.header}>
          <Avatar
            seed={state.name}
            label={groupInitials(
              state.name === "" ? copy.groupFallbackName : state.name,
            )}
            size={avatarSizes.newGroup}
          />
          <Text variant="subheadline" color="themeSecondaryLabel">
            {copy.makeItYours}
          </Text>
        </View>
      ) : null}
      {state.group ? (
        <FormSection>
          <TextFieldRow
            value={state.name}
            onChangeText={(text) => {
              store.setName(text);
            }}
            placeholder={copy.groupName}
            accessibilityLabel={copy.groupName}
          />
          <FormRow style={styles.descriptionRow}>
            <TextInput
              value={state.description}
              onChangeText={(text) => {
                store.setDescription(text);
              }}
              placeholder={copy.groupDescription}
              accessibilityLabel={copy.groupDescription}
              placeholderTextColor={palette.placeholderText}
              selectionColor={palette.tint}
              multiline
              scrollEnabled={false}
              style={[
                fontOf("body"),
                styles.description,
                { color: palette.label },
              ]}
            />
          </FormRow>
        </FormSection>
      ) : null}
      <FormSection
        style={state.group ? styles.headed : styles.firstHeaded}
        header={copy.members(memberCount(state))}>
        {state.group ? <MemberRow key="you" /> : null}
        {state.recipients.map((person) => (
          <MemberRow key={person.inboxId} person={person} />
        ))}
        <FormRow key="change">
          <GlassButton
            title={copy.changePeople}
            onPress={() => {
              router.back();
            }}
          />
        </FormRow>
      </FormSection>
      {state.group ? (
        <FormSection
          style={styles.headed}
          header={copy.permissionsHeader}
          footer={copy.permissionsFooter}>
          <PermissionRow
            title={copy.allMembers}
            detail={copy.allMembersDetail}
            selected={!state.adminOnly}
            onPress={() => {
              store.setAdminOnly(false);
            }}
          />
          <PermissionRow
            title={copy.adminsOnly}
            detail={copy.adminsOnlyDetail}
            selected={state.adminOnly}
            onPress={() => {
              store.setAdminOnly(true);
            }}
          />
        </FormSection>
      ) : null}
      <LoadingAndError state={state} store={store} showError />
    </FormScreen>
  );
}

/**
 * The picker's checkmark: 14.7 pt wide, its right edge 20.3 pt inside the
 * section (measured on iOS 27), smaller than a body symbol.
 */
const CHECKMARK_SIZE = 18.5;

/** One choice of the inline "Group permissions" picker. */
function PermissionRow({
  title,
  detail,
  selected,
  onPress,
}: {
  readonly title: string;
  readonly detail: string;
  readonly selected: boolean;
  readonly onPress: () => void;
}) {
  return (
    <FormRow
      onPress={onPress}
      accessibilityLabel={`${title}, ${detail}`}
      accessibilityState={{ selected }}>
      <View style={styles.permissionText}>
        <Text>{title}</Text>
        <Text variant="caption" color="themeSecondaryLabel">
          {detail}
        </Text>
      </View>
      {/* The picker keeps the checkmark column on every row. */}
      <Symbol
        name="checkmark"
        size={CHECKMARK_SIZE}
        color="tint"
        weight="semibold"
        style={[styles.checkmark, selected ? undefined : styles.hidden]}
      />
    </FormRow>
  );
}

const styles = StyleSheet.create({
  content: { paddingTop: 12 },
  // The clear header row, measured on iOS 27.
  header: { alignItems: "center", gap: 12, paddingBottom: 7, paddingTop: 15.3 },
  // Header sections sit 1.3 pt closer than the shared kit default here.
  headed: { paddingTop: 15.1 },
  firstHeaded: { paddingTop: 10 },
  descriptionRow: { paddingVertical: 0 },
  description: {
    flex: 1,
    minHeight: 52,
    paddingBottom: 15,
    paddingTop: 15,
  },
  permissionText: { flex: 1, gap: 4 },
  checkmark: { marginRight: 2.5 },
  hidden: { opacity: 0 },
});
