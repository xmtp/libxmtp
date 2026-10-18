import { router, useFocusEffect } from "expo-router";
import { useCallback, useEffect, useRef, useState } from "react";
import { StyleSheet, View } from "react-native";

import { type FileDraft, AttachmentCopy } from "@/core/attachments.ts";
import { createAttachmentStore } from "@/features/attachments/attachmentServices";
import { recoveryCopy } from "@/features/attachments/copy";
import { RecoveryChatsStore } from "@/features/attachments/recoveryChatsStore";
import { type ActiveSession, useActiveSession } from "@/session";
import { FormRow, FormScreen, FormSection } from "@/ui/Form";
import { GlassButton } from "@/ui/GlassButton";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { useStore } from "@/ui/useStore";

function draftStatus(draft: FileDraft): string {
  if (draft.phase === "unknown") return recoveryCopy.unknown;
  if (draft.conversationId === undefined) return recoveryCopy.unassigned;
  return draft.phase.charAt(0).toUpperCase() + draft.phase.slice(1);
}

/**
 * Attachment recovery (`AttachmentRecoveryFeatureView`): every saved upload
 * draft, including SDK uploads no chat claims, with View chat, Assign to a
 * chat, and Discard. Sends are not possible here.
 */
export function AttachmentRecoveryScreen() {
  const session = useActiveSession();
  if (session === undefined) return <FormScreen />;
  return <RecoveryList key={String(session.generation)} session={session} />;
}

function RecoveryList({ session }: { readonly session: ActiveSession }) {
  const mounted = useRef(true);
  const [store] = useState(() =>
    createAttachmentStore(session, {
      admit: () => mounted.current,
      allowed: () => false,
    }),
  );
  const [choices] = useState(
    () => new RecoveryChatsStore(session, () => mounted.current),
  );
  const state = useStore(store);
  const { chats } = useStore(choices);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useFocusEffect(
    useCallback(() => {
      void store.load();
    }, [store]),
  );
  const needsChats = state.drafts.some(
    (draft) => draft.conversationId === undefined,
  );
  useEffect(() => {
    if (needsChats) void choices.load();
  }, [needsChats, choices]);

  return (
    <FormScreen testID="attachment-recovery">
      <FormSection header={recoveryCopy.header}>
        <FormRow>
          <Text color="secondaryLabel">{recoveryCopy.intro}</Text>
        </FormRow>
      </FormSection>
      {state.drafts.map((draft) => (
        <FormSection key={draft.id} testID={`recovery.${draft.id}`}>
          <FormRow separatorInset={52}>
            <View style={styles.label}>
              <View style={styles.labelIcon}>
                <Symbol name="doc" color="tint" />
              </View>
              <Text style={styles.labelText}>
                {draft.file.filename ?? AttachmentCopy.file}
              </Text>
            </View>
          </FormRow>
          <FormRow>
            <Text variant="caption" color="secondaryLabel">
              {draftStatus(draft)}
            </Text>
          </FormRow>
          {draft.conversationId === undefined ? (
            chats.map((chat) => (
              <FormRow key={chat.id}>
                <GlassButton
                  title={recoveryCopy.assignTo(chat.title)}
                  testID={`recovery.assign.${chat.id}`}
                  onPress={() => {
                    store.assign(draft.id, chat.id);
                  }}
                />
              </FormRow>
            ))
          ) : (
            <FormRow>
              <GlassButton
                title={recoveryCopy.viewChat}
                testID="recovery.view-chat"
                onPress={() => {
                  router.push({
                    pathname: "/chat/[conversationId]",
                    params: { conversationId: draft.conversationId ?? "" },
                  });
                }}
              />
            </FormRow>
          )}
          <FormRow>
            <GlassButton
              title={recoveryCopy.discard}
              role="destructive"
              testID="recovery.discard"
              onPress={() => void store.discard(draft.id)}
            />
          </FormRow>
        </FormSection>
      ))}
      {state.drafts.length === 0 ? (
        <FormSection>
          <FormRow>
            <Text testID="recovery.empty">{recoveryCopy.empty}</Text>
          </FormRow>
        </FormSection>
      ) : null}
      {state.error === undefined ? null : (
        <FormSection>
          <FormRow>
            <Text color="systemRed">{state.error}</Text>
          </FormRow>
        </FormSection>
      )}
    </FormScreen>
  );
}

const styles = StyleSheet.create({
  // Like a plain NavigationRow: a 24 pt icon column, then 12 pt to the title.
  label: { alignItems: "center", flexDirection: "row", gap: 12 },
  labelIcon: { alignItems: "center", minWidth: 24 },
  labelText: { flexShrink: 1 },
});
