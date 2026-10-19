import { router } from "expo-router";
import { useEffect, useState } from "react";
import { StyleSheet, View } from "react-native";

import { personInitials } from "@/core/avatar.ts";
import type { Appearance } from "@/core/sessionIndex.ts";
import { settingsCopy as copy } from "@/features/settings/copy";
import { SendReviewSection } from "@/features/settings/sendReviewSlot";
import { createSettingsModel } from "@/features/settings/settingsStore";
import { copyText } from "@/platform/clipboard";
import {
  type ActiveSession,
  useActiveSession,
  useAppearance,
  useSessionState,
} from "@/session";
import { AlertDialog, type AlertDialogRequest } from "@/ui/AlertDialog";
import { Avatar } from "@/ui/Avatar";
import { FormRow, FormScreen, FormSection } from "@/ui/Form";
import { ButtonRow, LabeledRow, NavigationRow } from "@/ui/FormRows";
import { HandleText } from "@/ui/Identity";
import { SegmentedPicker } from "@/ui/SegmentedPicker";
import { Text } from "@/ui/Text";
import { avatarSizes } from "@/ui/tokens";
import { useStore } from "@/ui/useStore";

/**
 * Settings: profile, server, notifications status, appearance, privacy, send
 * review, developer tools and this device. Sign out and deletion confirm with
 * system alerts; the session router then shows Connect.
 */
export function SettingsScreen() {
  const session = useActiveSession();
  if (session === undefined) return null;
  return <Settings key={session.generation} session={session} />;
}

/** The avatar row's separator starts under the initials (16 + (64 - glyph) / 2). */
const profileSeparatorInset = 41.5;

function Settings({ session }: { readonly session: ActiveSession }) {
  const [model] = useState(createSettingsModel);
  useEffect(() => {
    model.appear();
    session.refreshOwnHandles().catch(() => {});
    return () => {
      model.dispose();
    };
  }, [model, session]);
  const state = useStore(model);
  const handles = useStore(session.ownHandles);
  const appearance = useAppearance();
  const sessionState = useSessionState();
  const server = session.server;
  const handle = handles.current;

  const [alert, setAlert] = useState<AlertDialogRequest>();
  const confirmSignOut = () => {
    setAlert({
      title: copy.signOutTitle,
      message: copy.signOutMessage,
      buttons: [
        {
          text: copy.signOut,
          style: "destructive",
          onPress: () => void model.signOut(),
        },
        { text: copy.cancel, style: "cancel" },
      ],
    });
  };
  const confirmDelete = () => {
    setAlert({
      title: copy.deleteTitle,
      message: copy.deleteMessage,
      buttons: [
        {
          text: copy.deleteLocalData,
          style: "destructive",
          onPress: () => void model.deleteLocalData(),
        },
        { text: copy.cancel, style: "cancel" },
      ],
    });
  };

  return (
    <>
      <FormScreen>
        <FormSection>
          <FormRow separatorInset={profileSeparatorInset}>
            <Avatar
              seed={session.inboxId}
              label={personInitials(handle ?? copy.you)}
              size={avatarSizes.settings}
            />
            <View style={styles.profileText}>
              {handle === undefined ? (
                <Text variant="headline">{copy.noHandle}</Text>
              ) : (
                <HandleText handle={handle} variant="title3" weight="bold" />
              )}
              <Text variant="caption" color="themeSecondaryLabel">
                {copy.handleCaption}
              </Text>
            </View>
          </FormRow>
          {handles.others.length === 0 ? null : (
            <FormRow>
              <Text variant="caption" color="themeSecondaryLabel">
                {copy.otherHandles}
              </Text>
            </FormRow>
          )}
          {handles.others.map((other) => (
            <FormRow key={other}>
              <HandleText handle={other} />
            </FormRow>
          ))}
          {handles.others.length === 0 ? null : (
            <FormRow>
              <Text
                variant="footnote"
                color="themeSecondaryLabel"
                style={styles.flex}>
                {copy.otherHandlesDetail}
              </Text>
            </FormRow>
          )}
          <ButtonRow
            title={copy.changeHandle}
            systemImage="at"
            navigates
            onPress={() => {
              router.push("/change-handle");
            }}
          />
          <LabeledRow
            layout="stacked"
            label={copy.inboxId}
            value={session.inboxId}
            valueVariant="footnote"
            monospaced
            truncate="tail"
            testID="profile-inbox-id"
          />
          <ButtonRow
            title={copy.copyInboxId}
            systemImage="doc.on.doc"
            onPress={() => void copyText(session.inboxId)}
          />
        </FormSection>
        <FormSection header={copy.server} footer={copy.serverFooter}>
          <LabeledRow label={copy.name} value={server.name} />
          <LabeledRow
            layout="stacked"
            label={copy.address}
            value={server.url}
            monospaced
            truncate="middle"
          />
          <ButtonRow
            title={copy.copyServerAddress}
            systemImage="doc.on.doc"
            onPress={() => void copyText(server.url)}
          />
        </FormSection>
        <FormSection
          header={copy.notifications}
          footer={copy.notificationsFooter}>
          <LabeledRow label={copy.status} value={copy.pushStatus} />
        </FormSection>
        <FormSection header={copy.appearance} footer={copy.appearanceFooter}>
          <FormRow padding={8}>
            <SegmentedPicker<Appearance>
              accessibilityLabel={copy.appearance}
              options={[
                { value: "system", label: copy.automatic },
                { value: "light", label: copy.light },
                { value: "dark", label: copy.dark },
              ]}
              selection={appearance}
              onSelectionChange={(value) => void model.setAppearance(value)}
            />
          </FormRow>
        </FormSection>
        <FormSection header={copy.privacy} footer={copy.privacyFooter}>
          <NavigationRow
            title={copy.messageRequests}
            systemImage="tray"
            icon="tile"
            iconColor="tint"
            onPress={() => {
              router.push("/requests");
            }}
          />
        </FormSection>
        <SendReviewSection session={session} />
        <FormSection header={copy.developer} footer={copy.developerFooter}>
          <NavigationRow
            title={copy.attachmentRecovery}
            systemImage="doc"
            icon="tile"
            iconColor="systemGray"
            onPress={() => {
              router.push("/attachment-recovery");
            }}
          />
        </FormSection>
        <FormSection header={copy.device} footer={copy.deviceFooter}>
          <ButtonRow
            title={copy.signOut}
            systemImage="rectangle.portrait.and.arrow.right"
            disabled={state.busy}
            onPress={confirmSignOut}
          />
          <ButtonRow
            title={copy.deleteLocalData}
            systemImage="trash"
            role="destructive"
            disabled={state.busy}
            onPress={confirmDelete}
          />
          {sessionState.kind === "resetIncomplete" ? (
            <FormRow>
              <Text>{copy.resetIncomplete}</Text>
            </FormRow>
          ) : null}
          {sessionState.kind === "resetIncomplete" ? (
            <ButtonRow
              title={copy.continueDeletion}
              systemImage="arrow.clockwise"
              onPress={() => void model.continueReset()}
            />
          ) : null}
        </FormSection>
        {state.error === undefined ? null : (
          <FormSection>
            <FormRow>
              <Text color="systemRed" style={styles.flex}>
                {state.error}
              </Text>
            </FormRow>
          </FormSection>
        )}
      </FormScreen>
      <AlertDialog
        request={alert}
        onDismiss={() => {
          setAlert(undefined);
        }}
      />
    </>
  );
}

const styles = StyleSheet.create({
  profileText: { flex: 1, gap: 3 },
  flex: { flex: 1 },
});
