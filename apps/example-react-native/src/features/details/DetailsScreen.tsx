import { Stack, router, useFocusEffect } from "expo-router";
import { useCallback, useEffect, useRef, useState } from "react";
import { ActivityIndicator, StyleSheet, View } from "react-native";

import {
  type ConversationDetails,
  type DetailsStore,
  initialLoadError,
  memberPending,
} from "@/core/details.ts";
import { canLeave } from "@/core/permissions.ts";
import { conversationTitle, detailsAvatarLabel } from "@/core/titles.ts";
import {
  detailsCopy,
  detailsSubtitle,
  membersHeader,
  retentionValue,
} from "@/features/details/copy";
import {
  DetailsErrorSection,
  DetailsNavigationRow,
  MemberIdentityRow,
  ShortcutTile,
} from "@/features/details/DetailsRows";
import { useSharedDetailsStore } from "@/features/details/detailsStores";
import { type ActiveSession, useActiveSession } from "@/session";
import { useIsAccessibilitySize } from "@/ui/accessibility";
import { Avatar } from "@/ui/Avatar";
import { showAlert, showConfirmation } from "@/ui/dialogs";
import { FormRow, FormScreen, FormSection } from "@/ui/Form";
import { ButtonRow, TextFieldRow } from "@/ui/FormRows";
import { GlassButton } from "@/ui/GlassButton";
import { ConversationTitle } from "@/ui/Identity";
import { Text } from "@/ui/Text";
import { useToolbarIcon } from "@/ui/toolbarIcon";
import { useStore } from "@/ui/useStore";

export interface DetailsScreenProps {
  readonly conversationId: string;
}

/**
 * Conversation details (`DetailsFeatureView`): an inset-grouped list with a
 * hidden title, the header with shortcut tiles, the local name, members,
 * shared media, group settings, developer tools, and Block / Leave. Groups
 * get an Edit toolbar button. It reloads each time it comes into view.
 */
export function DetailsScreen({ conversationId }: DetailsScreenProps) {
  const session = useActiveSession();
  if (session === undefined) {
    return <FormScreen>{titleOptions()}</FormScreen>;
  }
  return (
    <DetailsBody
      key={session.generation}
      session={session}
      conversationId={conversationId}
    />
  );
}

/** The bar keeps "Details" for accessibility but draws no title. */
function titleOptions() {
  return (
    <Stack.Screen
      options={{
        title: detailsCopy.title,
        headerTitleStyle: { color: "transparent" },
      }}
    />
  );
}

function DetailsBody({
  session,
  conversationId,
}: {
  readonly session: ActiveSession;
  readonly conversationId: string;
}) {
  const store = useSharedDetailsStore(session, conversationId);
  const state = useStore(store);
  const accessibilitySize = useIsAccessibilitySize();
  const pencil = useToolbarIcon("pencil");
  const [alias, setAlias] = useState(
    () => session.preferences.alias(conversationId) ?? "",
  );
  const aliasRef = useRef(alias);
  aliasRef.current = alias;

  const saveAlias = useCallback(() => {
    if (!session.isCurrent()) return;
    try {
      session.preferences.setAlias(conversationId, aliasRef.current);
    } catch {
      showAlert(detailsCopy.saveAliasFailed);
    }
  }, [session, conversationId]);

  useFocusEffect(
    useCallback(() => {
      setAlias(session.preferences.alias(conversationId) ?? "");
      store.send({ kind: "refresh" });
      return saveAlias;
    }, [session, conversationId, store, saveAlias]),
  );

  useEffect(() => {
    if (state.blocked) router.dismissTo("/chats");
  }, [state.blocked]);

  const details = state.details;
  if (details === undefined) {
    const error = initialLoadError(state);
    return (
      <FormScreen>
        {titleOptions()}
        {error === undefined ? (
          <FormSection>
            <FormRow style={styles.center}>
              <ActivityIndicator />
            </FormRow>
          </FormSection>
        ) : (
          <FormSection>
            <FormRow>
              <Text
                color="systemRed"
                testID="details.error"
                style={styles.flex}>
                {error}
              </Text>
            </FormRow>
            <FormRow>
              <GlassButton
                variant="glassProminent"
                title={detailsCopy.retry}
                onPress={() => {
                  store.send({ kind: "refresh" });
                }}
              />
            </FormRow>
          </FormSection>
        )}
      </FormScreen>
    );
  }

  const openEdit = () => {
    router.push({
      pathname: "/edit-group/[conversationId]",
      params: { conversationId },
    });
  };
  const push = (
    pathname:
      | "/shared-media/[conversationId]"
      | "/permissions/[conversationId]"
      | "/disappearing/[conversationId]"
      | "/group-fields/[conversationId]"
      | "/my-fields/[conversationId]",
  ) => {
    router.push({ pathname, params: { conversationId } });
  };

  return (
    <FormScreen
      automaticallyAdjustKeyboardInsets
      contentContainerStyle={styles.content}>
      {titleOptions()}
      {details.kind === "group" ? (
        <Stack.Toolbar placement="right">
          {accessibilitySize ? (
            <Stack.Toolbar.Button
              icon={pencil}
              accessibilityLabel={detailsCopy.edit}
              onPress={openEdit}
            />
          ) : (
            <Stack.Toolbar.Button onPress={openEdit}>
              {detailsCopy.edit}
            </Stack.Toolbar.Button>
          )}
        </Stack.Toolbar>
      ) : null}
      <Header conversationId={conversationId} details={details} alias={alias} />
      <FormSection
        header={detailsCopy.nameOnThisDevice}
        footer={detailsCopy.nameFooter}>
        <TextFieldRow
          placeholder={detailsCopy.localName}
          value={alias}
          onChangeText={setAlias}
          onSubmitEditing={saveAlias}
          onEndEditing={saveAlias}
          returnKeyType="done"
        />
      </FormSection>
      <FormSection>
        <FormRow>
          <Text style={styles.flex}>{detailsCopy.notificationsOff}</Text>
        </FormRow>
      </FormSection>
      <MembersSection
        conversationId={conversationId}
        details={details}
        store={store}
      />
      <FormSection header={detailsCopy.sharedMedia}>
        <ButtonRow
          title={detailsCopy.mediaAndFiles}
          systemImage="photo"
          iconStyle="label"
          iconSize={photoIconSize}
          onPress={() => {
            push("/shared-media/[conversationId]");
          }}
        />
      </FormSection>
      <FormSection header={detailsCopy.groupSettings}>
        {details.kind === "group" ? (
          <DetailsNavigationRow
            title={detailsCopy.permissions}
            systemImage="lock.shield"
            value={details.preset}
            onPress={() => {
              push("/permissions/[conversationId]");
            }}
          />
        ) : null}
        <DetailsNavigationRow
          title={detailsCopy.disappearingMessages}
          systemImage="hourglass"
          value={retentionValue(details.retentionNs)}
          onPress={() => {
            push("/disappearing/[conversationId]");
          }}
        />
      </FormSection>
      <FormSection
        header={detailsCopy.developerTools}
        footer={detailsCopy.developerFooter}>
        {details.kind === "group" ? (
          <DetailsNavigationRow
            title={detailsCopy.groupFields}
            systemImage="curlybraces"
            onPress={() => {
              push("/group-fields/[conversationId]");
            }}
          />
        ) : null}
        <DetailsNavigationRow
          title={detailsCopy.myFields}
          systemImage="person.crop.square"
          onPress={() => {
            push("/my-fields/[conversationId]");
          }}
        />
      </FormSection>
      <FormSection footer={detailsCopy.localRemovalFootnote}>
        <FormRow
          onPress={() => {
            showConfirmation({
              confirm: detailsCopy.block,
              destructive: true,
              onConfirm: () => {
                store.send({ kind: "mutate", mutation: { kind: "block" } });
              },
            });
          }}>
          <Text style={styles.flex}>{detailsCopy.block}</Text>
        </FormRow>
        {canLeave(details, store.ownInboxId) ? (
          <FormRow
            onPress={() => {
              showConfirmation({
                confirm: detailsCopy.leave,
                destructive: true,
                onConfirm: () => {
                  store.send({ kind: "mutate", mutation: { kind: "leave" } });
                },
              });
            }}>
            <Text style={styles.flex}>{detailsCopy.leave}</Text>
          </FormRow>
        ) : null}
        {state.pendingLeave ? (
          <FormRow>
            <Text style={styles.flex}>{detailsCopy.membershipPending}</Text>
          </FormRow>
        ) : null}
      </FormSection>
      <DetailsErrorSection error={state.error} />
    </FormScreen>
  );
}

function Header({
  conversationId,
  details,
  alias,
}: {
  readonly conversationId: string;
  readonly details: ConversationDetails;
  readonly alias: string;
}) {
  const accessibilitySize = useIsAccessibilitySize();
  const openAddMember = () => {
    router.push({
      pathname: "/add-member/[conversationId]",
      params: { conversationId },
    });
  };
  return (
    <FormSection plain>
      <View style={styles.header}>
        <Avatar
          seed={conversationId}
          label={detailsAvatarLabel(details.record, alias)}
          size={96}
        />
        <ConversationTitle
          title={conversationTitle(details.record, alias)}
          variant="title"
          weight="bold"
          align="center"
        />
        <Text variant="subheadline" color="secondaryLabel" align="center">
          {detailsSubtitle(
            details.kind,
            details.members.length,
            details.record.peerHandle,
          )}
        </Text>
        {details.description === "" ? null : (
          <Text color="secondaryLabel" align="center">
            {details.description}
          </Text>
        )}
        <View
          style={[
            styles.tiles,
            accessibilitySize ? styles.tilesStacked : undefined,
          ]}>
          {details.kind === "group" ? (
            <ShortcutTile
              title={detailsCopy.addMember}
              systemImage="person.badge.plus"
              testID="details.add-member-shortcut"
              onPress={openAddMember}
            />
          ) : null}
          <ShortcutTile
            title={detailsCopy.disappearing}
            systemImage="hourglass"
            onPress={() => {
              router.push({
                pathname: "/disappearing/[conversationId]",
                params: { conversationId },
              });
            }}
          />
        </View>
      </View>
    </FormSection>
  );
}

function MembersSection({
  conversationId,
  details,
  store,
}: {
  readonly conversationId: string;
  readonly details: ConversationDetails;
  readonly store: DetailsStore;
}) {
  const state = useStore(store);
  const pending = [...state.pendingMembers.values()].sort((a, b) =>
    a.id < b.id ? -1 : a.id > b.id ? 1 : 0,
  );
  return (
    <FormSection
      header={membersHeader(details.members.length)}
      footer={detailsCopy.membersFooter}>
      {details.kind === "group" ? (
        <ButtonRow
          title={detailsCopy.addMember}
          systemImage="person.badge.plus"
          color="systemBlue"
          iconColor="tint"
          iconStyle="label"
          iconSize={24}
          testID="details.add-member-row"
          onPress={() => {
            router.push({
              pathname: "/add-member/[conversationId]",
              params: { conversationId },
            });
          }}
        />
      ) : null}
      {details.members.flatMap((member) => [
        <MemberIdentityRow
          key={member.id}
          member={member}
          testID={`details.member.${member.id}`}
          onPress={() => {
            router.push({
              pathname: "/member/[conversationId]/[inboxId]",
              params: { conversationId, inboxId: member.id },
            });
          }}
        />,
        ...(memberPending(state, member.id)
          ? [
              <FormRow key={`${member.id}.pending`}>
                <Text variant="caption" color="secondaryLabel">
                  {detailsCopy.membershipPending}
                </Text>
              </FormRow>,
            ]
          : []),
      ])}
      {pending.map((member) => (
        <MemberIdentityRow
          key={`pending.${member.id}`}
          member={member}
          trailing={<Text variant="caption">{detailsCopy.pending}</Text>}
        />
      ))}
    </FormSection>
  );
}

/** The list `Label` draws `photo` 24 x 19 pt (measured on iOS 27). */
const photoIconSize = 29;

const styles = StyleSheet.create({
  content: { paddingBottom: 16 },
  flex: { flex: 1 },
  center: { justifyContent: "center" },
  header: { alignItems: "center", gap: 10 },
  tiles: { alignSelf: "stretch", flexDirection: "row", gap: 10, paddingTop: 6 },
  tilesStacked: { flexDirection: "column" },
});
