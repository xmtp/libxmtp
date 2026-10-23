import { router } from "expo-router";
import { useEffect, useRef, useState } from "react";
import { type LayoutChangeEvent, StyleSheet, View } from "react-native";

import type { AttachmentStore } from "@/core/attachmentStore.ts";
import { type ChatMessage, messageDate } from "@/core/chatMessages.ts";
import { messageDateTime } from "@/core/dates.ts";
import { errorLogText } from "@/core/errorLog.ts";
import { SharedMediaStore, mediaSections } from "@/core/sharedMedia.ts";
import { splitHandle, stripAt } from "@/core/titles.ts";
import { AttachmentCard } from "@/features/attachments/AttachmentCard";
import {
  consentAllowed,
  createAttachmentStore,
} from "@/features/attachments/attachmentServices";
import { sharedMediaCopy } from "@/features/attachments/copy";
import { mediaMessages } from "@/features/attachments/sdkAttachmentGateway";
import { chatServices } from "@/features/chat/chatServices";
import { type ActiveSession, useActiveSession } from "@/session";
import { FormRow, FormScreen, FormSection } from "@/ui/Form";
import { GlassButton } from "@/ui/GlassButton";
import { IdentityText } from "@/ui/Identity";
import { Span, Text } from "@/ui/Text";
import { useStore } from "@/ui/useStore";

/** An adaptive grid: columns at least 140 pt wide, 12 pt apart. */
const GRID_MINIMUM = 140;
const GRID_SPACING = 12;

/** Columns of an adaptive grid `width` wide, and each column's width. */
function gridColumns(width: number): { count: number; width: number } {
  const count = Math.max(
    1,
    Math.floor((width + GRID_SPACING) / (GRID_MINIMUM + GRID_SPACING)),
  );
  return { count, width: (width - GRID_SPACING * (count - 1)) / count };
}

/**
 * Media and files: the 50 newest file messages, "Photos and videos" in an
 * adaptive grid and other "Files" in rows, each with its sender, date, file
 * card and "View in chat".
 */
export function SharedMediaScreen({
  conversationId,
}: {
  readonly conversationId: string;
}) {
  const session = useActiveSession();
  if (session === undefined) return <FormScreen />;
  return (
    <SharedMediaList
      key={`${String(session.generation)}:${conversationId}`}
      session={session}
      conversationId={conversationId}
    />
  );
}

function SharedMediaList({
  session,
  conversationId,
}: {
  readonly session: ActiveSession;
  readonly conversationId: string;
}) {
  const mounted = useRef(true);
  const [media] = useState(
    () =>
      new SharedMediaStore(
        conversationId,
        {
          conversation: (id) => chatServices(session).queries.conversation(id),
          media: (id) => mediaMessages(session.client, id),
        },
        () => mounted.current && session.isCurrent(),
      ),
  );
  const [files] = useState(() =>
    createAttachmentStore(session, {
      conversationId,
      admit: () => mounted.current,
      allowed: () => media.getState().allowed,
      authorizeDownload: consentAllowed(session, conversationId),
    }),
  );
  const state = useStore(media);
  const identities = useStore(session.identities);
  const [failure, setFailure] = useState<string>();
  const [gridWidth, setGridWidth] = useState(0);

  useEffect(() => {
    mounted.current = true;
    void media.load();
    const stop = session.streams.onEvent((event) => {
      if (event.kind !== "changed") return;
      files.invalidateDownloads();
      void media.load();
    });
    return () => {
      mounted.current = false;
      stop();
    };
  }, [media, files, session]);

  useEffect(() => {
    const missing = [...new Set(state.rows.map((row) => row.sender))].filter(
      (sender) => session.identities.identity(sender) === undefined,
    );
    if (missing.length === 0) return;
    session.identities.load(missing).catch((error: unknown) => {
      console.warn("Cannot read sender handles", errorLogText(error));
    });
  }, [state.rows, session]);

  const viewInChat = (row: ChatMessage) => {
    try {
      if (row.position !== undefined) {
        session.preferences.setScrollAnchor(conversationId, {
          messageId: row.id,
          sentAtNs: row.position.sentAtNs,
          cursor: row.position.cursor,
        });
      }
      router.push({
        pathname: "/chat/[conversationId]",
        params: { conversationId },
      });
    } catch {
      setFailure(sharedMediaCopy.anchorFailed);
    }
  };

  const { visual, files: others } = mediaSections(state.rows);
  const grid = gridColumns(gridWidth);
  const gridRows: ChatMessage[][] = [];
  for (let index = 0; index < visual.length; index += grid.count) {
    gridRows.push(visual.slice(index, index + grid.count));
  }
  const card = (row: ChatMessage) => (
    <MediaCard
      row={row}
      store={files}
      allowed={state.allowed}
      outgoing={row.sender === session.inboxId}
      handle={identities.get(row.sender)?.handle}
      onViewInChat={() => {
        viewInChat(row);
      }}
    />
  );

  return (
    <FormScreen testID="shared-media" contentContainerStyle={styles.inlineTop}>
      <FormSection>
        <FormRow>
          <Text color="secondaryLabel">{sharedMediaCopy.intro}</Text>
        </FormRow>
      </FormSection>
      <FormSection header={sharedMediaCopy.photos}>
        <FormRow>
          <View
            style={styles.grid}
            onLayout={(event: LayoutChangeEvent) => {
              setGridWidth(event.nativeEvent.layout.width);
            }}>
            {gridWidth === 0
              ? null
              : gridRows.map((items) => (
                  <View key={items[0]?.id} style={styles.gridRow}>
                    {items.map((row) => (
                      <View key={row.id} style={{ width: grid.width }}>
                        {card(row)}
                      </View>
                    ))}
                  </View>
                ))}
          </View>
        </FormRow>
      </FormSection>
      <FormSection header={sharedMediaCopy.files}>
        {others.map((row) => (
          <FormRow key={row.id}>{card(row)}</FormRow>
        ))}
      </FormSection>
      {state.error === undefined && failure === undefined ? null : (
        <FormSection>
          <FormRow>
            <Text color="systemRed" testID="shared-media.error">
              {state.error ?? failure}
            </Text>
          </FormRow>
          {state.error === undefined ? null : (
            // A failed load keeps downloads closed until Retry loads again.
            <FormRow>
              <GlassButton
                title={sharedMediaCopy.retry}
                testID="shared-media.retry"
                onPress={() => void media.load()}
              />
            </FormRow>
          )}
        </FormSection>
      )}
      <FormSection>
        <FormRow>
          <Text variant="caption" color="secondaryLabel">
            {sharedMediaCopy.limit}
          </Text>
        </FormRow>
      </FormSection>
    </FormScreen>
  );
}

function MediaCard({
  row,
  store,
  allowed,
  outgoing,
  handle,
  onViewInChat,
}: {
  readonly row: ChatMessage;
  readonly store: AttachmentStore;
  readonly allowed: boolean;
  readonly outgoing: boolean;
  readonly handle: string | undefined;
  readonly onViewInChat: () => void;
}) {
  if (row.content.kind !== "attachment") return null;
  return (
    <View style={styles.mediaCard} testID={`media.${row.id}`}>
      <View style={styles.byline}>
        <SenderText inboxId={row.sender} handle={handle} />
        <Text variant="caption" color="secondaryLabel" style={styles.date}>
          {messageDateTime(messageDate(row))}
        </Text>
      </View>
      <AttachmentCard
        store={store}
        messageId={row.id}
        file={row.content.file}
        allowed={allowed}
        appearance="list"
        outgoing={outgoing}
        metadata={`${handle ?? row.sender.slice(0, 12)} · ${messageDateTime(messageDate(row))}`}
      />
      <GlassButton
        title={sharedMediaCopy.viewInChat}
        testID={`media.view-in-chat.${row.id}`}
        onPress={onViewInChat}
      />
    </View>
  );
}

/**
 * The byline's `IdentityText` in secondary text: a handle's name takes the
 * secondary color and its `#code` the tertiary one; an inbox ID prefix stays
 * monospaced.
 */
function SenderText({
  inboxId,
  handle,
}: {
  readonly inboxId: string;
  readonly handle: string | undefined;
}) {
  if (handle === undefined || handle === "") {
    return (
      <IdentityText
        inboxId={inboxId}
        variant="caption"
        color="secondaryLabel"
        style={styles.sender}
      />
    );
  }
  const { name, code } = splitHandle(handle);
  return (
    <Text
      variant="caption"
      color="secondaryLabel"
      accessibilityLabel={stripAt(handle)}
      style={styles.sender}>
      {name}
      {code === "" ? null : <Span color="tertiaryLabel">{code}</Span>}
    </Text>
  );
}

const styles = StyleSheet.create({
  // An inline-title list starts 35 pt below the bar (the form kit's insets are
  // measured for large titles).
  inlineTop: { paddingTop: 35 },
  grid: { flex: 1, gap: GRID_SPACING },
  gridRow: { alignItems: "center", flexDirection: "row", gap: GRID_SPACING },
  mediaCard: { alignItems: "flex-start", gap: 8 },
  // One line that never wraps; a text wraps in place. Only the date wraps:
  // the one-word sender keeps its width, where Yoga would shrink both and
  // split the handle.
  byline: { alignItems: "center", flexDirection: "row", gap: 8 },
  date: { flexShrink: 1 },
  sender: { flexShrink: 0, maxWidth: "100%" },
});
