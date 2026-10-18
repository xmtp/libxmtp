import { Stack, router } from "expo-router";
import { useEffect, useRef, useState } from "react";
import { ActivityIndicator, StyleSheet, View } from "react-native";

import {
  type DetailsMember,
  type DetailsState,
  type DetailsStore,
  initialLoadError,
  memberPending,
} from "@/core/details.ts";
import {
  canChangeRole,
  canRemoveMember,
  ROLE_LABELS,
} from "@/core/permissions.ts";
import { memberInitial } from "@/core/titles.ts";
import { DetailsErrorSection } from "@/features/details/DetailsRows";
import { useOwnDetailsStore } from "@/features/details/detailsStores";
import { useSheetFormProps } from "@/features/details/sheetDetent";
import { memberCopy } from "@/features/member/copy";
import { copyText } from "@/platform/clipboard";
import { type ActiveSession, useActiveSession } from "@/session";
import { Avatar } from "@/ui/Avatar";
import { useColors } from "@/ui/colors";
import { ContentUnavailable } from "@/ui/ContentUnavailable";
import { FormRow, FormScreen, FormSection } from "@/ui/Form";
import { ButtonRow, LabeledRow } from "@/ui/FormRows";
import { IdentityText } from "@/ui/Identity";
import {
  PopoverConfirmation,
  type PopoverConfirmationRequest,
} from "@/ui/PopoverConfirmation";
import { Text } from "@/ui/Text";
import { useToolbarIcon } from "@/ui/toolbarIcon";
import { useStore } from "@/ui/useStore";

/** A list `Label` draws its symbol at about 27 pt. */
const labelIconSize = 27;
/** A 26 pt box draws `doc.on.doc` at its measured 21 x 26 pt. */
const copyIconSize = 26;

export interface MemberScreenProps {
  readonly conversationId: string;
  readonly inboxId: string;
}

/**
 * The Member sheet (`MemberFeatureView`): identity and role, the inbox ID
 * with Copy, and Make/Remove admin and Remove from group when the group's
 * policies allow them. It closes after a confirmed removal.
 */
export function MemberScreen({ conversationId, inboxId }: MemberScreenProps) {
  const session = useActiveSession();
  const form = useSheetFormProps();
  if (session === undefined) {
    return (
      <>
        <MemberToolbar />
        <FormScreen {...form} />
      </>
    );
  }
  return (
    <MemberBody
      key={session.generation}
      session={session}
      conversationId={conversationId}
      inboxId={inboxId}
    />
  );
}

function MemberToolbar() {
  const palette = useColors();
  const close = useToolbarIcon("xmark");
  return (
    <>
      <Stack.Screen options={{ title: memberCopy.title }} />
      <Stack.Toolbar placement="left">
        <Stack.Toolbar.Button
          icon={close}
          tintColor={palette.label}
          accessibilityLabel={memberCopy.close}
          onPress={() => {
            router.back();
          }}
        />
      </Stack.Toolbar>
    </>
  );
}

/** Dismisses the sheet once a requested removal is confirmed by the group. */
function useCloseAfterRemoval(state: DetailsState, inboxId: string) {
  const requested = useRef(false);
  useEffect(() => {
    const details = state.details;
    if (
      !requested.current ||
      state.busy ||
      state.error !== undefined ||
      details === undefined ||
      details.members.some((member) => member.id === inboxId) ||
      state.pendingRemoval.has(inboxId)
    ) {
      return;
    }
    requested.current = false;
    router.back();
  }, [state, inboxId]);
  return requested;
}

function MemberBody({
  session,
  conversationId,
  inboxId,
}: {
  readonly session: ActiveSession;
  readonly conversationId: string;
  readonly inboxId: string;
}) {
  const form = useSheetFormProps();
  const store = useOwnDetailsStore(session, conversationId);
  const state = useStore(store);
  const removal = useCloseAfterRemoval(state, inboxId);
  const [confirmation, setConfirmation] = useState<
    PopoverConfirmationRequest | undefined
  >();
  const details = state.details;
  const member = details?.members.find((entry) => entry.id === inboxId);

  if (details !== undefined && member !== undefined) {
    return (
      <>
        <MemberToolbar />
        <FormScreen {...form}>
          <MemberHeader member={member} />
          <FormSection header={memberCopy.inboxId}>
            <LabeledRow
              label={memberCopy.otherHandles}
              value={
                member.otherHandles.length === 0
                  ? memberCopy.noOtherHandles
                  : member.otherHandles.join(", ")
              }
            />
            <FormRow>
              <Text
                variant="footnote"
                monospaced
                numberOfLines={1}
                ellipsizeMode="middle"
                selectable
                style={styles.flex}>
                {member.id}
              </Text>
            </FormRow>
            <ButtonRow
              title={memberCopy.copyInboxId}
              systemImage="doc.on.doc"
              color="label"
              iconColor="tint"
              iconStyle="label"
              iconSize={copyIconSize}
              onPress={() => void copyText(member.id)}
            />
          </FormSection>
          <MemberActions
            store={store}
            member={member}
            onRemove={() => {
              setConfirmation({
                title: memberCopy.removeQuestion,
                confirm: memberCopy.remove,
                destructive: true,
                onConfirm: () => {
                  removal.current = true;
                  store.send({
                    kind: "mutate",
                    mutation: { kind: "remove", id: member.id },
                  });
                },
              });
            }}
          />
          {memberPending(state, inboxId) ? (
            <FormSection>
              <FormRow>
                <Text style={styles.flex}>{memberCopy.membershipPending}</Text>
              </FormRow>
            </FormSection>
          ) : null}
          <DetailsErrorSection error={state.error} />
        </FormScreen>
        {/* SwiftUI anchors the form's `.confirmationDialog` to the form. */}
        <PopoverConfirmation
          request={confirmation}
          onDismiss={() => {
            setConfirmation(undefined);
          }}
        />
      </>
    );
  }
  if (details !== undefined && !state.busy) {
    return (
      <>
        <MemberToolbar />
        <FormScreen {...form} contentContainerStyle={styles.fill}>
          <ContentUnavailable
            title={memberCopy.gone}
            systemImage="person.crop.circle.badge.xmark"
          />
          <DetailsErrorSection error={state.error} />
        </FormScreen>
      </>
    );
  }
  const error = initialLoadError(state);
  return (
    <>
      <MemberToolbar />
      <FormScreen {...form}>
        {error === undefined ? (
          <FormSection>
            <FormRow style={styles.center}>
              <ActivityIndicator />
            </FormRow>
          </FormSection>
        ) : (
          <DetailsErrorSection error={error} />
        )}
      </FormScreen>
    </>
  );
}

/** Avatar 52, the identity in title3 semibold and a blue role capsule. */
function MemberHeader({ member }: { readonly member: DetailsMember }) {
  return (
    <FormSection plain style={styles.headerSection}>
      <View style={styles.header}>
        <Avatar
          seed={member.id}
          label={memberInitial(member.id, member.handle)}
          size={52}
        />
        <View style={styles.identity}>
          <IdentityText
            inboxId={member.id}
            handle={member.handle}
            variant="title3"
            weight="semibold"
          />
          <View style={styles.badgeRow}>
            <BlueBadge text={ROLE_LABELS[member.role]} />
          </View>
        </View>
      </View>
    </FormSection>
  );
}

function BlueBadge({ text }: { readonly text: string }) {
  const palette = useColors();
  return (
    <View style={[styles.badge, { backgroundColor: palette.blueFill10 }]}>
      <Text variant="caption" weight="medium" color="systemBlue">
        {text}
      </Text>
    </View>
  );
}

function MemberActions({
  store,
  member,
  onRemove,
}: {
  readonly store: DetailsStore;
  readonly member: DetailsMember;
  readonly onRemove: () => void;
}) {
  const state = useStore(store);
  const details = state.details;
  if (details === undefined) return null;
  const own = store.ownInboxId;
  const role = canChangeRole(details, member, own);
  const remove = canRemoveMember(details, member, own);
  if (!role && !remove) return null;
  return (
    <FormSection footer={memberCopy.actionsFooter}>
      {role ? (
        <ButtonRow
          title={
            member.role === "admin"
              ? memberCopy.removeAdmin
              : memberCopy.makeAdmin
          }
          systemImage="person.badge.key"
          color="label"
          iconColor="tint"
          iconStyle="label"
          iconSize={labelIconSize}
          onPress={() => {
            store.send({
              kind: "mutate",
              mutation: {
                kind: "admin",
                id: member.id,
                enabled: member.role !== "admin",
              },
            });
          }}
        />
      ) : null}
      {remove ? (
        <ButtonRow
          title={memberCopy.remove}
          systemImage="person.fill.xmark"
          role="destructive"
          iconStyle="label"
          iconSize={labelIconSize}
          testID="member.remove"
          onPress={onRemove}
        />
      ) : null}
    </FormSection>
  );
}

const styles = StyleSheet.create({
  flex: { flex: 1 },
  fill: { flexGrow: 1 },
  center: { justifyContent: "center" },
  // An inset-grouped List (not a Form) starts its first section lower.
  headerSection: { paddingTop: 35 },
  header: {
    alignItems: "center",
    flexDirection: "row",
    gap: 14,
    paddingHorizontal: 16,
    paddingVertical: 21,
  },
  identity: { flex: 1, gap: 6 },
  badgeRow: { flexDirection: "row" },
  badge: {
    borderRadius: 999,
    paddingHorizontal: 8,
    paddingVertical: 4,
  },
});
