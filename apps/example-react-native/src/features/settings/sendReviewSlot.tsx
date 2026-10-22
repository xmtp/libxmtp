import { router, useFocusEffect } from "expo-router";
import { useCallback, useEffect, useState } from "react";
import { StyleSheet, View } from "react-native";

import { messageDateTime } from "@/core/dates.ts";
import {
  type SendDraft,
  SendReviewCopy,
  canRetryDeletion,
  reviewDetail,
  reviewTitle,
} from "@/core/sendJournal.ts";
import { chatCopy } from "@/features/chat/copy";
import { SendReviewStore } from "@/features/chat/sendReviewStore";
import type { ActiveSession } from "@/session";
import { FormRow, FormSection } from "@/ui/Form";
import { GlassButton } from "@/ui/GlassButton";
import { Text } from "@/ui/Text";
import { useStore } from "@/ui/useStore";

export interface SendReviewSectionProps {
  readonly session: ActiveSession;
}

/**
 * Settings › "Send needs review": sends with an unknown outcome and deletions
 * that are not confirmed, each with View chat, Retry deletion (accepted
 * deletions) and Clear. Clearing never resends.
 * Settings renders `<SendReviewSection session={session} />` directly in its
 * `FormScreen`; the section brings its own `FormSection`.
 */
export function SendReviewSection({ session }: SendReviewSectionProps) {
  return <SendReviewRows key={String(session.generation)} session={session} />;
}

function SendReviewRows({ session }: { readonly session: ActiveSession }) {
  const [store] = useState(() => new SendReviewStore(session));
  useEffect(() => () => store.dispose(), [store]);
  useFocusEffect(
    useCallback(() => {
      store.reload();
    }, [store]),
  );
  const state = useStore(store);
  const { drafts } = state;
  return (
    <FormSection
      header={chatCopy.sendReviewTitle}
      testID="settings.send-review">
      {drafts.length === 0 ? (
        <FormRow>
          <Text color="secondaryLabel">{SendReviewCopy.empty}</Text>
        </FormRow>
      ) : null}
      {drafts.map((draft) => (
        <FormRow key={draft.id} testID={`send-review.${draft.id}`}>
          <ReviewEntry
            draft={draft}
            busy={state.retrying.has(draft.id)}
            onRetry={() => {
              store.retryDeletion(draft);
            }}
            onClear={() => {
              store.clear(draft);
            }}
          />
        </FormRow>
      ))}
      {drafts.length === 0 ? null : (
        <FormRow>
          <Text variant="caption" color="secondaryLabel">
            {SendReviewCopy.explanation}
          </Text>
        </FormRow>
      )}
      {drafts.some((draft) => draft.deletionTargetId !== undefined) ? (
        <FormRow>
          <Text variant="caption" color="secondaryLabel">
            {SendReviewCopy.deletionExplanation}
          </Text>
        </FormRow>
      ) : null}
      {state.error === undefined ? null : (
        <FormRow>
          <Text color="systemRed">{state.error}</Text>
        </FormRow>
      )}
    </FormSection>
  );
}

function ReviewEntry({
  draft,
  busy,
  onRetry,
  onClear,
}: {
  readonly draft: SendDraft;
  readonly busy: boolean;
  readonly onRetry: () => void;
  readonly onClear: () => void;
}) {
  const isDeletion = draft.deletionTargetId !== undefined;
  return (
    <View style={styles.entry}>
      <Text variant="headline">{reviewTitle(draft)}</Text>
      <Text variant="caption" color="secondaryLabel">
        {reviewDetail(draft)}
      </Text>
      <Text variant="caption">
        {messageDateTime(new Date(draft.createdAt))}
      </Text>
      <GlassButton
        title={SendReviewCopy.viewChat}
        testID="send-review.view-chat"
        onPress={() => {
          router.push({
            pathname: "/chat/[conversationId]",
            params: { conversationId: draft.conversationId },
          });
        }}
      />
      {canRetryDeletion(draft) ? (
        <GlassButton
          title={SendReviewCopy.retryDeletion}
          disabled={busy}
          testID="send-review.retry-deletion"
          onPress={onRetry}
        />
      ) : null}
      <GlassButton
        title={
          isDeletion ? SendReviewCopy.clearDeletion : SendReviewCopy.clearSend
        }
        disabled={busy}
        testID="send-review.clear"
        onPress={onClear}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  entry: { alignItems: "flex-start", gap: 8 },
});
