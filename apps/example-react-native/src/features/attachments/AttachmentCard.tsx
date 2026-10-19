import type { RemoteAttachment } from "@xmtp/react-native-sdk";
import { Image } from "expo-image";
import { useEffect, useRef, useState } from "react";
import { ActivityIndicator, StyleSheet, View } from "react-native";

import { byteCount, cardStatus } from "@/core/attachments.ts";
import type { AttachmentStore } from "@/core/attachmentStore.ts";
import { pathToUri } from "@/core/paths.ts";
import { attachmentCopy } from "@/features/attachments/copy";
import {
  exportCopy,
  previewFile,
  removeExport,
  saveFile,
} from "@/platform/attachmentFiles";
import { useActiveSession } from "@/session";
import type { ColorName } from "@/ui/colors";
import { GlassButton } from "@/ui/GlassButton";
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
}

/** Card width 260 minus its 12 pt padding on both sides. */
const PREVIEW_WIDTH = 236;
const PREVIEW_HEIGHT = 220;

const palettes: Record<
  CardAppearance,
  {
    readonly text: ColorName;
    readonly secondary: ColorName;
    readonly icon: ColorName;
    readonly button?: ColorName;
  }
> = {
  incoming: {
    text: "label",
    secondary: "secondaryLabel",
    icon: "label",
    button: "label",
  },
  outgoing: {
    text: "onTint",
    secondary: "onTint",
    icon: "onTint",
    button: "onTint",
  },
  list: { text: "label", secondary: "secondaryLabel", icon: "tint" },
};

/**
 * A file message: name, size and status, then Download (or Retry), a spinner
 * while downloading, or Open and Save once the file is verified. Verified
 * images show a preview above the name.
 */
export function AttachmentCard({
  store,
  messageId,
  file,
  allowed,
  appearance,
}: AttachmentCardProps) {
  const session = useActiveSession();
  // Re-render on store changes; `card` reads the current state.
  useStore(store);
  const card = store.card(messageId, file);
  const verified =
    card.phase === "verified" ? store.verified(messageId) : undefined;
  const [failure, setFailure] = useState<string>();
  const [copying, setCopying] = useState(false);
  const exported = useRef<string | undefined>(undefined);
  const mounted = useRef(true);
  // Read through a function: the flag changes across awaits.
  const alive = () => mounted.current;
  const colors = palettes[appearance];

  const clearExport = () => {
    if (!takeExport(exported) && mounted.current) {
      setFailure(attachmentCopy.removeFailed);
    }
  };

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      takeExport(exported);
    };
  }, []);

  const open = async (save: boolean) => {
    const source = store.verified(messageId);
    if (!allowed || copying || source === undefined || session === undefined) {
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
      if (mounted.current) {
        setFailure(attachmentCopy.openFailed);
        setCopying(false);
      }
      return;
    }
    if (!mounted.current || !session.isCurrent()) {
      removeExport(path);
      return;
    }
    exported.current = path;
    try {
      if (save) {
        await saveFile(path, card.name);
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
  };

  const error = card.error ?? failure;
  return (
    <View style={styles.card}>
      {verified?.mimeType?.startsWith("image/") === true ? (
        <Preview path={verified.path} />
      ) : null}
      <View
        style={[
          styles.label,
          appearance === "list" ? styles.listLabel : undefined,
        ]}>
        {appearance === "list" ? (
          // A list row styles its label: a larger icon in its own column.
          <View style={styles.listIcon}>
            <Symbol
              name="doc.fill"
              textStyle="subheadline"
              size={LIST_ICON}
              color={colors.icon}
            />
          </View>
        ) : (
          <Symbol name="doc.fill" textStyle="subheadline" color={colors.icon} />
        )}
        <Text
          variant="subheadline"
          weight="medium"
          color={colors.text}
          style={styles.name}>
          {card.name}
        </Text>
      </View>
      <View
        style={[
          styles.meta,
          appearance === "outgoing" ? styles.outgoingSecondary : undefined,
        ]}>
        {card.length === undefined ? null : (
          <Text variant="caption" color={colors.secondary}>
            {byteCount(card.length)}
          </Text>
        )}
        <Text variant="caption" color={colors.secondary} style={styles.status}>
          {cardStatus(card.phase, allowed)}
        </Text>
      </View>
      {card.phase === "verified" ? (
        <View style={styles.actions}>
          <GlassButton
            title={attachmentCopy.open}
            disabled={!allowed || copying}
            {...(colors.button === undefined
              ? {}
              : { labelColor: colors.button })}
            testID={`attachment.open.${messageId}`}
            onPress={() => void open(false)}
          />
          <GlassButton
            title={attachmentCopy.save}
            disabled={!allowed || copying}
            {...(colors.button === undefined
              ? {}
              : { labelColor: colors.button })}
            testID={`attachment.save.${messageId}`}
            onPress={() => void open(true)}
          />
        </View>
      ) : card.phase === "downloading" ? (
        <ActivityIndicator
          accessibilityLabel={attachmentCopy.downloading}
          style={styles.spinner}
        />
      ) : allowed && (card.phase !== "failed" || card.retryable) ? (
        <GlassButton
          title={
            card.phase === "failed"
              ? attachmentCopy.retry
              : attachmentCopy.download
          }
          {...(colors.button === undefined
            ? {}
            : { labelColor: colors.button })}
          testID={`attachment.download.${messageId}`}
          onPress={() => void store.download(messageId, file)}
        />
      ) : null}
      {error === undefined ? null : (
        <Text variant="caption" color="systemRed">
          {error}
        </Text>
      )}
    </View>
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

/** A verified image, fit within 236 x 220 pt with 12 pt corners. */
function Preview({ path }: { readonly path: string }) {
  const [size, setSize] = useState<{ width: number; height: number }>();
  // The card's content width (236 pt in a bubble, less in a grid column).
  const [available, setAvailable] = useState(PREVIEW_WIDTH);
  const aspect =
    size === undefined || size.height === 0
      ? undefined
      : size.width / size.height;
  const width =
    aspect === undefined ? 0 : Math.min(available, PREVIEW_HEIGHT * aspect);
  return (
    <View
      style={styles.previewFrame}
      onLayout={(event) => {
        setAvailable(event.nativeEvent.layout.width);
      }}>
      <Image
        source={{ uri: pathToUri(path) }}
        contentFit="contain"
        // Decrypted files never leave protected storage: no disk cache copy.
        cachePolicy="memory"
        accessible={false}
        onLoad={(event) => {
          setSize({ width: event.source.width, height: event.source.height });
        }}
        style={[
          styles.preview,
          aspect === undefined
            ? styles.previewLoading
            : { width, height: width / aspect },
        ]}
      />
    </View>
  );
}

/** The list label's `doc.fill` box (subheadline, large image scale). */
const LIST_ICON = 23.5;

const styles = StyleSheet.create({
  // Fills up to 260 pt when the parent offers more.
  card: {
    alignItems: "flex-start",
    gap: 8,
    maxWidth: "100%",
    padding: 12,
    width: 260,
  },
  label: { alignItems: "center", flexDirection: "row", gap: 8 },
  // The title starts 36 pt after the icon's leading edge, and the icon is
  // about 20 pt tall.
  listLabel: { gap: 17 },
  listIcon: { alignItems: "center", minWidth: 19 },
  name: { flexShrink: 1 },
  meta: { alignItems: "center", flexDirection: "row", gap: 8 },
  status: { flexShrink: 1 },
  outgoingSecondary: { opacity: 0.7 },
  actions: { flexDirection: "row", flexWrap: "wrap", gap: 8 },
  spinner: { alignSelf: "flex-start" },
  preview: { borderCurve: "continuous", borderRadius: 12, overflow: "hidden" },
  previewFrame: { alignSelf: "stretch" },
  previewLoading: { height: 1, width: 1 },
});
