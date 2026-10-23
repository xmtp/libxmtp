import type { RemoteAttachment } from "@xmtp/react-native-sdk";
import { Image } from "expo-image";
import { useFocusEffect } from "expo-router";
import { use, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ActivityIndicator, Pressable, StyleSheet, View } from "react-native";
import Animated from "react-native-reanimated";

import { byteCount, cardStatus } from "@/core/attachments.ts";
import type { AttachmentStore } from "@/core/attachmentStore.ts";
import { pathToUri } from "@/core/paths.ts";
import {
  AttachmentActions,
  type AttachmentAction,
} from "@/features/attachments/AttachmentActions";
import {
  attachmentCopy,
  attachmentPresentationCopy,
} from "@/features/attachments/copy";
import { MessageContextMenu } from "@/features/chat/interactions/MessageContextMenu";
import {
  exportCopy,
  previewFile,
  removeExport,
  saveFile,
} from "@/platform/attachmentFiles";
import { useActiveSession } from "@/session";
import { AlertDialog } from "@/ui/AlertDialog";
import { type ColorName, useColors } from "@/ui/colors";
import { useMotion } from "@/ui/motion";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { useStore } from "@/ui/useStore";

/** Where the card sits: inside an incoming or outgoing bubble, or in a list. */
export type CardAppearance = "incoming" | "outgoing" | "list";

export interface AttachmentCardProps {
  readonly store: AttachmentStore;
  readonly messageId: string;
  readonly file: RemoteAttachment;
  /** The conversation's consent is allowed ("Closed until you accept"). */
  readonly allowed: boolean;
  readonly appearance: CardAppearance;
  readonly metadata?: string;
  readonly outgoing?: boolean;
}

const palettes: Record<
  CardAppearance,
  {
    readonly text: ColorName;
    readonly secondary: ColorName;
    readonly icon: ColorName;
  }
> = {
  incoming: {
    text: "label",
    secondary: "secondaryLabel",
    icon: "label",
  },
  outgoing: {
    text: "onTint",
    secondary: "onTint",
    icon: "onTint",
  },
  list: { text: "label", secondary: "secondaryLabel", icon: "tint" },
};

/** An image fills its bubble; files use a compact row. Actions live in the message menu. */
export function AttachmentCard({
  store,
  messageId,
  file,
  allowed,
  appearance,
  metadata,
  outgoing = appearance === "outgoing",
}: AttachmentCardProps) {
  const session = useActiveSession();
  const palette = useColors();
  const motion = useMotion();
  const messageMenu = use(AttachmentActions);
  const [openRequest, setOpenRequest] = useState(0);
  const [showsMetadata, setShowsMetadata] = useState(false);
  const [details, setDetails] = useState(false);
  const [dimensions, setDimensions] = useState<string>();
  // Re-render on store changes; `card` reads the current state.
  const state = useStore(store);
  const card = store.card(messageId, file);
  const verified =
    card.phase === "verified" ? store.verified(messageId) : undefined;
  const [failure, setFailure] = useState<string>();
  const [copying, setCopying] = useState(false);
  const exported = useRef<string | undefined>(undefined);
  const mounted = useRef(true);
  // Read through a function: the flag changes across awaits.
  const alive = useCallback(() => mounted.current, []);
  const colors = palettes[appearance];

  const clearExport = useCallback(() => {
    if (!takeExport(exported) && mounted.current) {
      setFailure(attachmentCopy.removeFailed);
    }
  }, []);

  useFocusEffect(
    useCallback(() => {
      const phase = card.phase;
      if (allowed && (phase === "available" || phase === "failed")) {
        void store.prepare(messageId, file, outgoing);
      }
    }, [allowed, card.phase, state, file, messageId, outgoing, store]),
  );

  // A covered chat stays mounted. Keep it in sync if Media and files
  // finishes a download before or after the user returns to the chat.
  useEffect(() => store.watchFile(messageId, file), [store, messageId, file]);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      takeExport(exported);
    };
  }, []);

  const open = useCallback(
    async (save: boolean) => {
      const source = store.verified(messageId);
      if (
        !mounted.current ||
        !allowed ||
        copying ||
        source === undefined ||
        session === undefined ||
        !session.isCurrent()
      ) {
        return;
      }
      clearExport();
      setFailure(undefined);
      setCopying(true);
      let path: string;
      try {
        path = await exportCopy(
          source.path,
          source.filename,
          session.paths.exports,
        );
      } catch {
        if (alive()) {
          setFailure(attachmentCopy.openFailed);
          setCopying(false);
        }
        return;
      }
      if (!alive() || !session.isCurrent()) {
        removeExport(path);
        return;
      }
      exported.current = path;
      try {
        if (save) {
          await saveFile(path, card.name, source.mimeType);
          clearExport();
        } else {
          // Quick Look keeps reading the copy: it is removed by the next Open
          // or Save, when the card goes away, or when the session ends.
          await previewFile(path, source.mimeType);
        }
      } catch {
        if (save) clearExport();
        if (alive()) {
          setFailure(
            save ? attachmentCopy.saveFailed : attachmentCopy.openFailed,
          );
        }
      } finally {
        if (alive()) setCopying(false);
      }
    },
    [
      allowed,
      copying,
      session,
      store,
      messageId,
      card.name,
      clearExport,
      alive,
    ],
  );

  const error = card.error ?? failure;
  const image = verified?.mimeType?.startsWith("image/") === true;
  const actions = useMemo<readonly AttachmentAction[]>(() => {
    const items: AttachmentAction[] = [];
    if (card.phase === "verified") {
      items.push(
        {
          id: `attachment.open.${messageId}`,
          title: attachmentCopy.open,
          symbol: "square.and.arrow.up",
          disabled: !allowed || copying,
          run: () => void open(false),
        },
        {
          id: `attachment.save.${messageId}`,
          title: attachmentCopy.save,
          symbol: "square.and.arrow.down",
          disabled: !allowed || copying,
          run: () => void open(true),
        },
      );
    } else if (
      allowed &&
      card.phase !== "downloading" &&
      (card.phase !== "failed" || card.retryable)
    ) {
      items.push({
        id: `attachment.download.${messageId}`,
        title:
          card.phase === "failed"
            ? attachmentCopy.retry
            : attachmentCopy.download,
        symbol: "arrow.down.circle",
        run: () => void store.download(messageId, file),
      });
    }
    items.push({
      id: `attachment.details.${messageId}`,
      title: attachmentPresentationCopy.details,
      symbol: "doc",
      run: () => setDetails(true),
    });
    return items;
  }, [
    allowed,
    card.phase,
    card.retryable,
    copying,
    file,
    messageId,
    open,
    store,
  ]);
  useEffect(() => {
    if (messageMenu === undefined) return;
    messageMenu.register(actions);
    return () => messageMenu.register([]);
  }, [actions, messageMenu]);
  const showMenu = () => {
    if (messageMenu === undefined) setOpenRequest((value) => value + 1);
    else messageMenu.open();
  };
  const detailRequest = useMemo(
    () =>
      details
        ? {
            title: card.name,
            message: [
              verified?.mimeType,
              card.length === undefined ? undefined : byteCount(card.length),
              dimensions,
              metadata,
              cardStatus(card.phase, allowed),
              error,
            ]
              .filter(Boolean)
              .join("\n"),
            buttons: [{ text: attachmentPresentationCopy.done }],
          }
        : undefined,
    [
      allowed,
      card.length,
      card.name,
      card.phase,
      details,
      dimensions,
      error,
      metadata,
      verified?.mimeType,
    ],
  );
  const content = (
    <View style={[styles.card, image ? styles.imageCard : styles.fileCard]}>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={
          image
            ? [
                attachmentPresentationCopy.image,
                showsMetadata ? metadata : undefined,
              ]
                .filter(Boolean)
                .join(", ")
            : card.name
        }
        accessibilityHint={
          image
            ? attachmentPresentationCopy.tapHint
            : attachmentPresentationCopy.fileHint
        }
        accessibilityActions={[
          { name: "actions", label: attachmentPresentationCopy.actions },
        ]}
        onAccessibilityAction={() => showMenu()}
        onLongPress={showMenu}
        delayLongPress={400}
        onPress={() =>
          image ? setShowsMetadata((value) => !value) : setDetails(true)
        }
        style={styles.target}>
        {image ? (
          <>
            <Preview path={verified.path} onDimensions={setDimensions} />
            {showsMetadata && metadata !== undefined ? (
              <Animated.View
                entering={motion.entering}
                exiting={motion.exiting}
                style={[
                  styles.overlay,
                  { backgroundColor: palette.secondarySystemGroupedBackground },
                ]}>
                <Text variant="caption" color="label">
                  {metadata}
                </Text>
              </Animated.View>
            ) : null}
          </>
        ) : (
          <View style={styles.fileRow}>
            <Symbol name="doc.fill" textStyle="title2" color={colors.icon} />
            <View style={styles.fileText}>
              <Text
                variant="subheadline"
                weight="medium"
                color={colors.text}
                numberOfLines={2}>
                {card.name}
              </Text>
              <Text variant="caption" color={colors.secondary}>
                {[
                  verified?.mimeType ??
                    (card.name.includes(".")
                      ? card.name.split(".").at(-1)?.toUpperCase()
                      : undefined),
                  card.length === undefined
                    ? undefined
                    : byteCount(card.length),
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </Text>
              {card.phase === "verified" ? null : (
                <Text variant="caption" color={colors.secondary}>
                  {cardStatus(card.phase, allowed)}
                </Text>
              )}
            </View>
            {card.phase === "downloading" || copying ? (
              <ActivityIndicator
                accessibilityLabel={attachmentCopy.downloading}
              />
            ) : null}
          </View>
        )}
      </Pressable>
      {error === undefined ? null : (
        <Text variant="caption" color="systemRed" style={styles.error}>
          {error}
        </Text>
      )}
      <AlertDialog
        request={detailRequest}
        onDismiss={() => setDetails(false)}
      />
    </View>
  );
  return messageMenu === undefined ? (
    <MessageContextMenu
      items={[]}
      attachmentActions={actions}
      openRequest={openRequest}
      previewText={image ? attachmentPresentationCopy.image : card.name}
      quickReactions={false}
      onItem={() => {}}
      onReact={() => {}}
      onMore={() => {}}>
      {content}
    </MessageContextMenu>
  ) : (
    content
  );
}

/** Removes the export copy in `ref`, if any. False when the removal failed. */
function takeExport(ref: { current: string | undefined }): boolean {
  const path = ref.current;
  ref.current = undefined;
  if (path === undefined) return true;
  try {
    removeExport(path);
    return true;
  } catch {
    return false;
  }
}

/** Full-width image with a bounded portrait height; no decrypted disk-cache copy. */
function Preview({
  path,
  onDimensions,
}: {
  readonly path: string;
  readonly onDimensions: (value: string) => void;
}) {
  const [aspect, setAspect] = useState(1);
  return (
    <Image
      source={{ uri: pathToUri(path) }}
      contentFit="cover"
      cachePolicy="memory"
      accessible={false}
      onLoad={(event) => {
        if (event.source.height > 0)
          setAspect(
            Math.max(
              0.7,
              Math.min(2, event.source.width / event.source.height),
            ),
          );
        onDimensions(
          `${String(event.source.width)} × ${String(event.source.height)}`,
        );
      }}
      style={[styles.preview, { aspectRatio: aspect }]}
    />
  );
}

const styles = StyleSheet.create({
  card: {
    maxWidth: "100%",
    borderRadius: 18,
    borderCurve: "continuous",
    overflow: "hidden",
  },
  imageCard: { width: 320 },
  fileCard: { width: 280 },
  target: { width: "100%" },
  fileRow: { alignItems: "center", flexDirection: "row", gap: 12, padding: 14 },
  fileText: { flex: 1, gap: 4 },
  preview: { width: "100%" },
  overlay: {
    position: "absolute",
    bottom: 8,
    left: 8,
    right: 8,
    paddingHorizontal: 10,
    paddingVertical: 6,
    borderRadius: 12,
  },
  error: { padding: 10 },
});
