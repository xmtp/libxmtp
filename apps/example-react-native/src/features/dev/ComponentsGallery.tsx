import { Stack } from "expo-router";
import { type ReactNode, useState } from "react";
import { ScrollView, StyleSheet, View } from "react-native";
import Animated from "react-native-reanimated";

import { galleryCopy } from "@/features/dev/copy";
import { Avatar } from "@/ui/Avatar";
import { Bubble, BubbleText, ReactionBadge } from "@/ui/Bubble";
import { avatarPalette, type ColorName, useColors } from "@/ui/colors";
import { ConnectionBanner } from "@/ui/ConnectionBanner";
import { ContentUnavailable } from "@/ui/ContentUnavailable";
import { showAlert, showConfirmation } from "@/ui/dialogs";
import { FormRow, FormSection } from "@/ui/Form";
import {
  ButtonRow,
  LabeledRow,
  NavigationRow,
  RadioRow,
  TextFieldRow,
  ToggleRow,
} from "@/ui/FormRows";
import { GlassButton, GlassIconButton } from "@/ui/GlassButton";
import { GlassSurface } from "@/ui/GlassSurface";
import { ConversationTitle, HandleText, IdentityText } from "@/ui/Identity";
import { useMotion } from "@/ui/motion";
import { PlainListRow } from "@/ui/PlainListRow";
import { SegmentedPicker } from "@/ui/SegmentedPicker";
import { SheetHeader } from "@/ui/SheetHeader";
import { Symbol, symbolNames, type SymbolName } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { avatarSizes } from "@/ui/tokens";
import { textStyles, type TextVariant } from "@/ui/typography";

const swatches: ColorName[] = [
  "tint",
  "accentText",
  "themeSecondaryLabel",
  "incomingBubble",
  "label",
  "secondaryLabel",
  "tertiaryLabel",
  "systemBackground",
  "systemGroupedBackground",
  "secondarySystemGroupedBackground",
  "separator",
  "systemRed",
  "systemGreen",
  "systemBlue",
  "systemGray",
  "tintFill15",
];

/** A titled block on the grouped background. */
function Block({ title, children }: { title: string; children: ReactNode }) {
  return (
    <View style={styles.block}>
      <Text variant="headline" color="secondaryLabel" style={styles.blockTitle}>
        {title}
      </Text>
      {children}
    </View>
  );
}

function Swatch({ name }: { name: ColorName }) {
  const palette = useColors();
  return (
    <View style={styles.swatch}>
      <View
        style={[
          styles.swatchChip,
          { backgroundColor: palette[name], borderColor: palette.separator },
        ]}
      />
      <Text variant="caption2" color="secondaryLabel" numberOfLines={1}>
        {name}
      </Text>
    </View>
  );
}

function MotionSample() {
  const motion = useMotion();
  const [shown, setShown] = useState(true);
  return (
    <View style={styles.motion}>
      <GlassButton
        title={galleryCopy.toggleMotion}
        onPress={() => {
          setShown((value) => !value);
        }}
      />
      <View style={styles.motionStage}>
        {shown ? (
          <Animated.View entering={motion.entering} exiting={motion.exiting}>
            <GlassSurface shape="capsule" style={styles.capsuleSample}>
              <Text variant="subheadline">
                {galleryCopy.motionSample}
                {motion.reduceMotion ? galleryCopy.reduceMotionSuffix : ""}
              </Text>
            </GlassSurface>
          </Animated.View>
        ) : null}
      </View>
    </View>
  );
}

/** Every design-system component in its states. */
export function ComponentsGallery() {
  const palette = useColors();
  const [field, setField] = useState("");
  const [toggle, setToggle] = useState(true);
  const [segment, setSegment] = useState<"a" | "b" | "c">("a");
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={{ backgroundColor: palette.systemGroupedBackground }}
      contentContainerStyle={styles.content}>
      <Stack.Screen
        options={{
          title: galleryCopy.sections.components,
          headerLargeTitleEnabled: false,
        }}
      />

      <Block title={galleryCopy.typography}>
        {(Object.keys(textStyles) as TextVariant[]).map((variant) => (
          <Text key={variant} variant={variant} numberOfLines={1}>
            {variant}
          </Text>
        ))}
        <Text variant="footnote" monospaced>
          21b5db7ac4ae9b8719f2b95d31c1a9b3
        </Text>
      </Block>

      <Block title={galleryCopy.colors}>
        <View style={styles.wrap}>
          {swatches.map((name) => (
            <Swatch key={name} name={name} />
          ))}
        </View>
      </Block>

      <Block title={galleryCopy.symbols}>
        <View style={styles.wrap}>
          {(Object.keys(symbolNames) as SymbolName[]).map((name) => (
            <View key={name} style={styles.symbolCell}>
              <Symbol name={name} color="tint" />
            </View>
          ))}
        </View>
      </Block>

      <Block title={galleryCopy.avatars}>
        <View style={styles.rowWrap}>
          {Object.values(avatarSizes).map((size) => (
            <Avatar key={size} seed="Protocol design" label="PD" size={size} />
          ))}
        </View>
        <View style={styles.rowWrap}>
          {["lena", "maya", "release", "9c", "protocol", "hike"].map((seed) => (
            <Avatar
              key={seed}
              seed={seed}
              label={seed === "9c" ? "9c" : seed.slice(0, 1).toUpperCase()}
              monospaced={seed === "9c"}
            />
          ))}
        </View>
        <Text variant="caption2" color="secondaryLabel">
          {avatarPalette.map((pair) => pair.background).join(" ")}
        </Text>
      </Block>

      <Block title={galleryCopy.identity}>
        <HandleText variant="headline" handle="marco#0412" />
        <IdentityText inboxId="3f9a21c0e4b17d2a9c" handle={null} />
        <IdentityText
          inboxId="3f9a21c0e4b17d2a9c"
          handle={null}
          short
          variant="caption2"
          color="secondaryLabel"
        />
        <ConversationTitle variant="headline" title="@marco#0412" />
        <ConversationTitle variant="headline" title="Protocol design" />
      </Block>

      <Block title={galleryCopy.buttons}>
        <GlassButton
          variant="glassProminent"
          size="large"
          fullWidth
          title={galleryCopy.joinServer}
          onPress={() => {}}
        />
        <GlassButton
          variant="glassProminent"
          size="large"
          fullWidth
          loading
          title={galleryCopy.joiningServer}
          onPress={() => {}}
        />
        <GlassButton
          variant="glassProminent"
          size="large"
          fullWidth
          disabled
          title={galleryCopy.joinServer}
          onPress={() => {}}
        />
        <View style={styles.rowWrap}>
          <GlassButton title={galleryCopy.retry} onPress={() => {}} />
          <GlassButton
            variant="glassProminent"
            title={galleryCopy.accept}
            onPress={() => {}}
          />
          <GlassButton
            role="destructive"
            title={galleryCopy.deleteRequest}
            onPress={() => {}}
          />
        </View>
        <View style={styles.rowWrap}>
          <GlassButton
            title={galleryCopy.block}
            systemImage="nosign"
            onPress={() => {}}
          />
          <GlassButton disabled title={galleryCopy.retry} onPress={() => {}} />
          <GlassIconButton
            systemImage="plus"
            accessibilityLabel={galleryCopy.addAttachment}
            onPress={() => {}}
          />
          <GlassIconButton
            systemImage="arrow.up"
            variant="glassProminent"
            accessibilityLabel={galleryCopy.send}
            onPress={() => {}}
          />
          <GlassIconButton
            systemImage="xmark"
            accessibilityLabel={galleryCopy.cancel}
            onPress={() => {}}
          />
        </View>
        <GlassButton
          fullWidth
          title={galleryCopy.senderDetails}
          onPress={() => {}}
        />
      </Block>

      <Block title={galleryCopy.surfaces}>
        <GlassSurface shape="capsule" style={styles.searchSample}>
          <Symbol name="magnifyingglass" />
          <Text color="tertiaryLabel">{galleryCopy.search}</Text>
        </GlassSurface>
        <View style={styles.rowWrap}>
          <GlassSurface cornerRadius={20} style={styles.tileSample}>
            <Symbol
              name="person.badge.plus"
              textStyle="title3"
              color="accentText"
            />
            <Text variant="caption" weight="semibold" color="accentText">
              {galleryCopy.addMember}
            </Text>
          </GlassSurface>
          <GlassSurface cornerRadius={20} style={styles.tileSample}>
            <Symbol name="hourglass" textStyle="title3" color="accentText" />
            <Text variant="caption" weight="semibold" color="accentText">
              {galleryCopy.disappearingShort}
            </Text>
          </GlassSurface>
        </View>
      </Block>

      <Block title={galleryCopy.banners}>
        <View style={styles.bleed}>
          <ConnectionBanner text={galleryCopy.reconnecting} />
          <ConnectionBanner text={galleryCopy.disconnected} />
        </View>
      </Block>

      <View>
        <FormSection header={galleryCopy.rows}>
          <NavigationRow
            title={galleryDetails.permissions}
            systemImage="lock.shield"
            value={galleryDetails.permissionsValue}
            onPress={() => {}}
          />
          <NavigationRow
            title={galleryCopy.settings.messageRequests}
            systemImage="tray"
            icon="tile"
            onPress={() => {}}
          />
          <ButtonRow
            title={galleryCopy.settings.changeHandle}
            systemImage="at"
            navigates
            onPress={() => {}}
          />
          <ButtonRow
            title={galleryCopy.settings.deleteLocalData}
            systemImage="trash"
            role="destructive"
            onPress={() => {}}
          />
          <LabeledRow
            label={galleryCopy.settings.name}
            value={galleryCopy.settings.serverUrl}
          />
          <LabeledRow
            layout="stacked"
            label={galleryCopy.settings.inboxId}
            value={galleryCopy.settings.inboxIdValue}
            valueVariant="footnote"
            monospaced
            truncate="tail"
          />
          <ToggleRow
            title={galleryCopy.notifications}
            value={toggle}
            onValueChange={setToggle}
          />
          <TextFieldRow
            placeholder={galleryCopy.durationPlaceholder}
            value={field}
            onChangeText={setField}
            keyboardType="number-pad"
          />
          <RadioRow
            title={galleryDetails.allMembers}
            detail={galleryDetails.allMembersDetail}
            selected
            onPress={() => {}}
          />
          <RadioRow
            title={galleryDetails.adminsOnly}
            detail={galleryDetails.adminsOnlyDetail}
            selected={false}
            onPress={() => {}}
          />
          <RadioRow
            indicator="checkmark"
            title={galleryDetails.oneHour}
            selected
            onPress={() => {}}
          />
          <FormRow padding={8}>
            <SegmentedPicker<"a" | "b" | "c">
              options={[
                { value: "a", label: galleryCopy.settings.automatic },
                { value: "b", label: galleryCopy.settings.light },
                { value: "c", label: galleryCopy.settings.dark },
              ]}
              selection={segment}
              onSelectionChange={setSegment}
            />
          </FormRow>
        </FormSection>
      </View>

      <Block title={galleryCopy.plainRows}>
        <View
          style={[styles.bleed, { backgroundColor: palette.systemBackground }]}>
          <PlainListRow
            showsTopSeparator
            separatorInset={72}
            onPress={() => {}}
            style={styles.chatRow}>
            <Avatar seed="Protocol design" label="PD" />
            <View style={styles.chatText}>
              <View style={styles.chatTitleLine}>
                <ConversationTitle
                  variant="headline"
                  title={galleryCopy.chats.rowTitle}
                  style={styles.flex}
                />
                <Text variant="caption" color="secondaryLabel">
                  {galleryCopy.chats.rowTime}
                </Text>
              </View>
              <Text
                variant="subheadline"
                color="secondaryLabel"
                numberOfLines={2}>
                {galleryCopy.chats.rowPreview}
              </Text>
            </View>
          </PlainListRow>
          <PlainListRow
            separatorInset={72}
            selected
            backgroundColor={palette.tintFill12}
            onPress={() => {}}
            style={styles.chatRow}>
            <Avatar seed="marco-inbox" label="M" />
            <View style={styles.chatText}>
              <ConversationTitle variant="headline" title="@marco#0412" />
              <Text
                variant="subheadline"
                color="secondaryLabel"
                numberOfLines={2}>
                {galleryCopy.chats.rowPreview}
              </Text>
            </View>
          </PlainListRow>
        </View>
      </Block>

      <Block title={galleryCopy.empty}>
        <ContentUnavailable
          fill={false}
          systemImage="exclamationmark.triangle"
          title={galleryCopy.sections.components}
          description={galleryCopy.sample}>
          <GlassButton title={galleryCopy.retry} onPress={() => {}} />
        </ContentUnavailable>
      </Block>

      <Block title={galleryCopy.sheetHeader}>
        <SheetHeader
          title={galleryCopy.newMessage}
          subtitle={galleryCopy.step1}
        />
      </Block>

      <Block title={galleryCopy.bubbles}>
        <View
          style={[
            styles.bubbles,
            { backgroundColor: palette.systemBackground },
          ]}>
          <View style={styles.incoming}>
            <View>
              <Bubble isOutgoing={false}>
                <BubbleText isOutgoing={false}>
                  {galleryCopy.chat.thereYouGo}
                </BubbleText>
              </Bubble>
              <View style={styles.badges}>
                <View style={styles.badgeTarget}>
                  <ReactionBadge emoji="👍" count={3} />
                </View>
                <View style={styles.badgeTarget}>
                  <ReactionBadge emoji="❤️" count={1} isSelected />
                </View>
              </View>
            </View>
          </View>
          <View style={styles.outgoing}>
            <Bubble isOutgoing>
              <BubbleText isOutgoing>{galleryCopy.chat.welcome}</BubbleText>
            </Bubble>
          </View>
          <View style={styles.outgoing}>
            <Bubble isOutgoing showsTail={false}>
              <BubbleText isOutgoing>{galleryCopy.chat.bringWine}</BubbleText>
            </Bubble>
          </View>
        </View>
      </Block>

      <Block title={galleryCopy.dialogs}>
        <View style={styles.rowWrap}>
          <GlassButton
            title={galleryCopy.showAlert}
            onPress={() => {
              showAlert(galleryCopy.alertTitle, galleryCopy.alertMessage, [
                { text: galleryCopy.signOut, style: "destructive" },
                { text: galleryCopy.cancel, style: "cancel" },
              ]);
            }}
          />
          <GlassButton
            title={galleryCopy.showConfirmation}
            onPress={() => {
              showConfirmation({
                confirm: galleryCopy.block,
                destructive: true,
                onConfirm: () => {},
              });
            }}
          />
        </View>
      </Block>

      <Block title={galleryCopy.motion}>
        <MotionSample />
      </Block>
    </ScrollView>
  );
}

const galleryDetails = galleryCopy.details;

const styles = StyleSheet.create({
  content: { gap: 24, paddingBottom: 48, paddingTop: 8 },
  block: { gap: 10, paddingHorizontal: 16 },
  blockTitle: { marginHorizontal: 16 },
  wrap: { flexDirection: "row", flexWrap: "wrap", gap: 8 },
  rowWrap: {
    alignItems: "center",
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 10,
  },
  swatch: { alignItems: "center", gap: 4, width: 82 },
  swatchChip: { borderRadius: 10, borderWidth: 1, height: 36, width: 60 },
  symbolCell: {
    alignItems: "center",
    height: 32,
    justifyContent: "center",
    width: 32,
  },
  searchSample: {
    alignItems: "center",
    flexDirection: "row",
    gap: 8,
    height: 48,
    paddingHorizontal: 16,
  },
  tileSample: {
    alignItems: "center",
    flex: 1,
    gap: 7,
    minHeight: 68,
    paddingHorizontal: 8,
    paddingVertical: 12,
  },
  bleed: { marginHorizontal: -16 },
  chatRow: { paddingVertical: 11 },
  chatText: { flex: 1, gap: 4 },
  chatTitleLine: { alignItems: "baseline", flexDirection: "row", gap: 8 },
  flex: { flex: 1 },
  bubbles: { borderRadius: 26, gap: 10, padding: 16 },
  incoming: { alignItems: "flex-start", paddingRight: 42, paddingVertical: 6 },
  outgoing: { alignItems: "flex-end", paddingLeft: 42 },
  // `reactionBadges`: 44 pt targets at the top-trailing corner, 12 pt up.
  badges: {
    flexDirection: "row",
    gap: 2,
    position: "absolute",
    right: 0,
    top: -12,
  },
  badgeTarget: {
    alignItems: "center",
    justifyContent: "center",
    minHeight: 44,
    minWidth: 44,
  },
  motion: { gap: 12 },
  motionStage: { alignItems: "flex-start", height: 48 },
  capsuleSample: { paddingHorizontal: 14, paddingVertical: 10 },
});
