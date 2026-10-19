import { router } from "expo-router";
import { ActivityIndicator, StyleSheet, View } from "react-native";

import { type FileDraft, draftMetadata } from "@/core/attachments.ts";
import type { AttachmentStore } from "@/core/attachmentStore.ts";
import { attachmentServices } from "@/features/attachments/attachmentServices";
import { useChatAttachments } from "@/features/attachments/chatAttachments";
import { attachmentCopy } from "@/features/attachments/copy";
import { useChatContext } from "@/features/chat/ChatContext";
import type { ComposerSlotProps } from "@/features/chat/ComposerSlots";
import {
  type PickedFile,
  StagingTooLargeError,
  pickFile,
  pickPhoto,
  removePickerCopy,
  stageBase64,
  stageCopy,
} from "@/platform/attachmentFiles";
import { launchEnvironment } from "@/platform/messengerPlatform";
import type { ActiveSession } from "@/session";
import { useIsAccessibilitySize } from "@/ui/accessibility";
import { useColors } from "@/ui/colors";
import { GlassButton, GlassIconButton } from "@/ui/GlassButton";
import { GlassMenuButton, type GlassMenuItem } from "@/ui/GlassMenuButton";
import { GlassSurface } from "@/ui/GlassSurface";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { useStore } from "@/ui/useStore";

/** The Debug "Test file" holds at most 64 KiB. */
const TEST_FILE_LIMIT = 65_536;

/** Picks, stages a protected copy within the limit, and hands it to the store. */
async function stagePicked(
  store: AttachmentStore,
  session: ActiveSession,
  pick: () => Promise<PickedFile | undefined>,
  failures: {
    readonly select: string;
    readonly copy: (error: unknown) => string;
  },
): Promise<void> {
  const limit = store.getState().limit;
  if (limit === undefined) return;
  let picked: PickedFile | undefined;
  try {
    picked = await pick();
  } catch {
    store.showError(failures.select);
    return;
  }
  if (picked === undefined) return;
  store.showError(undefined);
  try {
    const path = await stageCopy(
      picked.uri,
      attachmentServices(session).staging,
      limit,
    );
    await store.stage(path, picked.name, picked.mimeType);
  } catch (error) {
    store.showError(failures.copy(error));
  } finally {
    removePickerCopy(picked.uri);
  }
}

/** Debug: stages `XMTP_UI_TEST_ATTACHMENT` (`name:base64`). */
async function stageTestFile(
  store: AttachmentStore,
  session: ActiveSession,
  value: string,
): Promise<void> {
  const limit = store.getState().limit;
  const separator = value.indexOf(":");
  if (limit === undefined || separator <= 0) return;
  const name = value.slice(0, separator);
  const safeName = name.slice(name.lastIndexOf("/") + 1);
  store.showError(undefined);
  try {
    const path = stageBase64(
      value.slice(separator + 1),
      attachmentServices(session).staging,
      limit,
      TEST_FILE_LIMIT,
    );
    await store.stage(path, safeName);
  } catch {
    store.showError(attachmentCopy.testFileFailed);
  }
}

/**
 * The "+" attach menu (glass, title3 plus, 44 pt, "Add attachment"): shown
 * when the server offers attachments and the conversation is allowed.
 */
export function AttachMenu({ allowed }: ComposerSlotProps) {
  const { runtime } = useChatContext();
  const store = useChatAttachments(runtime);
  const { limit } = useStore(store);
  if (limit === undefined || !allowed) return null;
  const session = runtime.session;
  const items: GlassMenuItem[] = [
    {
      title: attachmentCopy.photos,
      systemImage: "photo",
      onPress: () =>
        void stagePicked(store, session, pickPhoto, {
          select: attachmentCopy.selectPhotoFailed,
          copy: () => attachmentCopy.selectPhotoFailed,
        }),
    },
    {
      title: attachmentCopy.files,
      systemImage: "folder",
      onPress: () =>
        void stagePicked(store, session, pickFile, {
          select: attachmentCopy.selectFileFailed,
          copy: (error) =>
            error instanceof StagingTooLargeError
              ? attachmentCopy.tooLarge
              : attachmentCopy.unreadable,
        }),
    },
  ];
  const testFile = __DEV__ ? launchEnvironment().uiTestAttachment : undefined;
  if (testFile !== undefined) {
    items.push({
      title: attachmentCopy.testFile,
      onPress: () => void stageTestFile(store, session, testFile),
    });
  }
  return (
    <GlassMenuButton
      systemImage="plus"
      accessibilityLabel={attachmentCopy.addAttachment}
      items={items}
      testID="composer.add-attachment"
    />
  );
}

/**
 * This chat's staged files and the attachment error line, which shows only
 * while the composer has no error of its own.
 */
export function StagedAttachments({
  conversationId,
  allowed,
}: ComposerSlotProps) {
  const { runtime } = useChatContext();
  const store = useChatAttachments(runtime);
  const state = useStore(store);
  const composerError = useStore(runtime.composer).error;
  const drafts = state.drafts.filter(
    (draft) => draft.conversationId === conversationId,
  );
  const error = composerError === undefined ? state.error : undefined;
  return (
    <>
      {drafts.map((draft) => (
        <StagedPreview
          key={draft.id}
          draft={draft}
          allowed={allowed}
          onSend={() => void store.send(draft.id)}
          onDiscard={() => void store.discard(draft.id)}
        />
      ))}
      {error === undefined ? null : (
        <Text
          variant="caption"
          color="secondaryLabel"
          align="center"
          testID="composer.attachment-error">
          {error}
        </Text>
      )}
    </>
  );
}

function StagedPreview({
  draft,
  allowed,
  onSend,
  onDiscard,
}: {
  readonly draft: FileDraft;
  readonly allowed: boolean;
  readonly onSend: () => void;
  readonly onDiscard: () => void;
}) {
  const palette = useColors();
  const stacked = useIsAccessibilitySize();
  const name = draft.file.filename ?? attachmentCopy.file;
  const details = (
    <View
      style={[styles.details, stacked ? undefined : styles.detailsInline]}
      accessible
      accessibilityLabel={[
        name,
        draftMetadata(draft),
        ...(draft.phase === "uploading" ? [attachmentCopy.uploading] : []),
      ].join(", ")}>
      <View style={styles.icon}>
        <View style={[styles.iconFill, { backgroundColor: palette.tint }]} />
        <Symbol name="doc.fill" textStyle="title2" color="tint" />
      </View>
      <View style={styles.text}>
        <Text variant="callout" weight="medium" numberOfLines={2}>
          {name}
        </Text>
        <Text variant="caption" color="secondaryLabel">
          {draftMetadata(draft)}
        </Text>
        {draft.phase === "uploading" ? (
          <Text variant="caption" color="secondaryLabel">
            {attachmentCopy.uploading}
          </Text>
        ) : null}
      </View>
    </View>
  );
  const actions = (
    <View style={[styles.actions, stacked ? styles.actionsStacked : undefined]}>
      {draft.phase === "unknown" ? (
        <GlassButton
          title={attachmentCopy.reviewSend}
          testID={`staged.review.${draft.id}`}
          onPress={() => {
            router.push("/attachment-recovery");
          }}
        />
      ) : draft.phase === "uploading" ? (
        <View style={styles.control}>
          <ActivityIndicator accessibilityLabel={attachmentCopy.uploading} />
        </View>
      ) : draft.phase !== "failed" || draft.retryable ? (
        <View style={styles.control}>
          <GlassIconButton
            systemImage="arrow.up"
            variant="glassProminent"
            shape="capsule"
            accessibilityLabel={
              draft.phase === "failed"
                ? attachmentCopy.retry
                : attachmentCopy.send
            }
            disabled={!allowed}
            testID={`staged.send.${draft.id}`}
            onPress={onSend}
          />
        </View>
      ) : null}
      <View style={styles.control}>
        <GlassIconButton
          systemImage="xmark"
          shape="capsule"
          accessibilityLabel={attachmentCopy.discard}
          testID={`staged.discard.${draft.id}`}
          onPress={onDiscard}
        />
      </View>
    </View>
  );
  return (
    <GlassSurface
      cornerRadius={16}
      testID={`staged.${draft.id}`}
      style={[styles.preview, stacked ? styles.previewStacked : undefined]}>
      {details}
      {actions}
    </GlassSurface>
  );
}

const styles = StyleSheet.create({
  preview: {
    alignItems: "center",
    flexDirection: "row",
    gap: 12,
    padding: 12,
  },
  previewStacked: { alignItems: "stretch", flexDirection: "column" },
  details: { alignItems: "center", flexDirection: "row", gap: 10 },
  detailsInline: { flex: 1 },
  // The symbol is not clipped: at accessibility sizes it outgrows the box.
  icon: {
    alignItems: "center",
    height: 44,
    justifyContent: "center",
    width: 44,
  },
  iconFill: {
    ...StyleSheet.absoluteFill,
    borderCurve: "continuous",
    borderRadius: 10,
    opacity: 0.1,
  },
  text: { flexShrink: 1, gap: 3 },
  actions: { alignItems: "center", flexDirection: "row", gap: 8 },
  actionsStacked: { alignSelf: "flex-end" },
  control: {
    alignItems: "center",
    justifyContent: "center",
    minHeight: 44,
    minWidth: 44,
  },
});
