import { Stack } from "expo-router";
import { useState } from "react";
import { StyleSheet, View } from "react-native";

import { galleryCopy } from "@/features/dev/copy";
import { Avatar } from "@/ui/Avatar";
import { FormRow, FormScreen, FormSection } from "@/ui/Form";
import { ButtonRow, LabeledRow, NavigationRow } from "@/ui/FormRows";
import { SegmentedPicker } from "@/ui/SegmentedPicker";
import { Text } from "@/ui/Text";

const copy = galleryCopy.settings;
type Appearance = "system" | "light" | "dark";

/**
 * Settings with sample content. Avatar row: the separator starts under the
 * initials, as SwiftUI aligns it to the row's first text (16 + (64 - glyph
 * width) / 2).
 */
export function SettingsReplica() {
  const [appearance, setAppearance] = useState<Appearance>("system");
  return (
    <FormScreen>
      <Stack.Screen
        options={{ title: copy.title, headerLargeTitleEnabled: true }}
      />
      <FormSection>
        <FormRow separatorInset={41.5}>
          <Avatar seed="9c" label="N" size={64} />
          <View style={styles.profileText}>
            <Text variant="title3" weight="bold">
              {copy.handle}
            </Text>
            <Text variant="caption" color="themeSecondaryLabel">
              {copy.handleCaption}
            </Text>
          </View>
        </FormRow>
        <ButtonRow
          title={copy.changeHandle}
          systemImage="at"
          navigates
          onPress={() => {}}
        />
        <LabeledRow
          layout="stacked"
          label={copy.inboxId}
          value={copy.inboxIdValue}
          valueVariant="footnote"
          monospaced
          truncate="tail"
          testID="profile-inbox-id"
        />
        <ButtonRow
          title={copy.copyInboxId}
          systemImage="doc.on.doc"
          onPress={() => {}}
        />
      </FormSection>
      <FormSection header={copy.server} footer={copy.serverFooter}>
        <LabeledRow label={copy.name} value={copy.serverUrl} />
        <LabeledRow
          layout="stacked"
          label={copy.address}
          value={copy.serverUrl}
          monospaced
          truncate="middle"
        />
        <ButtonRow
          title={copy.copyServerAddress}
          systemImage="doc.on.doc"
          onPress={() => {}}
        />
      </FormSection>
      <FormSection
        header={copy.notifications}
        footer={copy.notificationsFooter}>
        <LabeledRow label={copy.status} value={copy.statusValue} />
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
            onSelectionChange={setAppearance}
          />
        </FormRow>
      </FormSection>
      <FormSection header={copy.privacy} footer={copy.privacyFooter}>
        <NavigationRow
          title={copy.messageRequests}
          systemImage="tray"
          icon="tile"
          iconColor="tint"
          onPress={() => {}}
        />
      </FormSection>
      <FormSection header={copy.sendReview}>
        <FormRow>
          <Text color="secondaryLabel">{copy.noSends}</Text>
        </FormRow>
      </FormSection>
      <FormSection header={copy.developer} footer={copy.developerFooter}>
        <NavigationRow
          title={copy.attachmentRecovery}
          systemImage="doc"
          icon="tile"
          iconColor="systemGray"
          onPress={() => {}}
        />
      </FormSection>
      <FormSection header={copy.device} footer={copy.deviceFooter}>
        <ButtonRow
          title={copy.signOut}
          systemImage="rectangle.portrait.and.arrow.right"
          onPress={() => {}}
        />
        <ButtonRow
          title={copy.deleteLocalData}
          systemImage="trash"
          role="destructive"
          onPress={() => {}}
        />
      </FormSection>
    </FormScreen>
  );
}

const styles = StyleSheet.create({
  profileText: { flex: 1, gap: 3 },
});
