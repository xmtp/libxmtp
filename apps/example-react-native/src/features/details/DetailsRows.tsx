import type { ReactNode } from "react";
import { Pressable, StyleSheet, View } from "react-native";

import type { DetailsMember } from "@/core/details.ts";
import { ROLE_LABELS } from "@/core/permissions.ts";
import { memberInitial } from "@/core/titles.ts";
import { Avatar } from "@/ui/Avatar";
import { FormRow, FormSection } from "@/ui/Form";
import { Chevron } from "@/ui/FormRows";
import { GlassSurface } from "@/ui/GlassSurface";
import { IdentityText } from "@/ui/Identity";
import { Symbol, type SymbolName } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { form } from "@/ui/tokens";

/** The member row separator starts at the identity text, after the avatar. */
const memberSeparatorInset = 32.5;

export interface MemberIdentityRowProps {
  readonly member: DetailsMember;
  readonly onPress?: () => void;
  /** Extra trailing content ("Found", "Pending"). */
  readonly trailing?: ReactNode;
  readonly testID?: string;
}

/**
 * `MemberIdentityRow`: a 44 pt avatar, the handle or short inbox ID, and the
 * role in secondary text, with 4 pt more vertical padding than a text row.
 */
export function MemberIdentityRow({
  member,
  onPress,
  trailing,
  testID,
}: MemberIdentityRowProps) {
  const role = ROLE_LABELS[member.role];
  return (
    <FormRow
      onPress={onPress}
      testID={testID}
      accessibilityLabel={`${member.handle ?? member.id}, ${role}`}
      separatorInset={memberSeparatorInset}
      style={styles.memberRow}>
      <Avatar
        seed={member.id}
        label={memberInitial(member.id, member.handle)}
      />
      <IdentityText
        inboxId={member.id}
        handle={member.handle}
        numberOfLines={1}
        style={styles.flex}
      />
      <Text variant="subheadline" color="secondaryLabel">
        {role}
      </Text>
      {trailing}
    </FormRow>
  );
}
MemberIdentityRow.separatorInset = (): number => memberSeparatorInset;

export interface DetailsNavigationRowProps {
  readonly title: string;
  readonly systemImage: SymbolName;
  readonly value?: string;
  readonly onPress: () => void;
  readonly testID?: string;
}

/**
 * `DetailsNavigationRow` inside a default List button: a system blue symbol
 * in a 24 pt column, the title in the tint, the value and chevron in lighter
 * tints (the button tints its `.primary` and `.secondary` styles).
 */
export function DetailsNavigationRow({
  title,
  systemImage,
  value,
  onPress,
  testID,
}: DetailsNavigationRowProps) {
  return (
    <FormRow
      onPress={onPress}
      testID={testID}
      accessibilityLabel={value === undefined ? title : `${title}, ${value}`}>
      <View style={styles.iconColumn}>
        <Symbol name={systemImage} color="systemBlue" />
      </View>
      <Text color="tint" style={styles.flex}>
        {title}
      </Text>
      {value === undefined ? null : (
        <Text color="tintSecondary" numberOfLines={1}>
          {value}
        </Text>
      )}
      <View style={styles.chevron}>
        <Chevron color="tintTertiary" />
      </View>
    </FormRow>
  );
}
DetailsNavigationRow.separatorInset = (): number =>
  form.rowInset + form.iconColumn + form.iconGap;

export interface MediaRowProps {
  readonly title: string;
  readonly onPress: () => void;
}

/**
 * "Media and files": a default List button `Label` with the `photo` symbol,
 * all in the tint. The symbol draws 24 x 19 pt, its left edge 15.4 pt into
 * the row; the title starts 56.4 pt in (measured on iOS 27).
 */
export function MediaRow({ title, onPress }: MediaRowProps) {
  return (
    <FormRow onPress={onPress} accessibilityLabel={title}>
      <View style={styles.mediaIcon}>
        <Symbol name="photo" size={29} color="tint" />
      </View>
      <Text color="tint" style={styles.flex}>
        {title}
      </Text>
    </FormRow>
  );
}

/** `DetailsErrorView`: red error text in its own row. */
export function DetailsErrorSection({ error }: { readonly error?: string }) {
  if (error === undefined) return null;
  return (
    <FormSection>
      <FormRow>
        <Text color="systemRed" testID="details.error" style={styles.flex}>
          {error}
        </Text>
      </FormRow>
    </FormSection>
  );
}

export interface ShortcutTileProps {
  readonly title: string;
  readonly systemImage: SymbolName;
  readonly onPress: () => void;
  readonly testID?: string;
}

/** A Details shortcut tile (`DetailsShortcutLabelStyle`): glass, radius 20. */
export function ShortcutTile({
  title,
  systemImage,
  onPress,
  testID,
}: ShortcutTileProps) {
  return (
    <Pressable
      testID={testID}
      onPress={onPress}
      style={styles.tileWrap}
      accessibilityRole="button"
      accessibilityLabel={title}>
      <GlassSurface cornerRadius={20} interactive style={styles.tile}>
        <Symbol name={systemImage} textStyle="title3" color="accentText" />
        <Text variant="caption" weight="semibold" color="accentText">
          {title}
        </Text>
      </GlassSurface>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  flex: { flex: 1 },
  // SwiftUI adds `.padding(.vertical, 4)` inside the 15 pt row inset.
  memberRow: { paddingVertical: 19 },
  iconColumn: { alignItems: "center", width: form.iconColumn },
  // The chevron glyph is narrower than its symbol box: SwiftUI puts the
  // glyph's right edge on the 16 pt row inset.
  chevron: { marginLeft: -2.2, marginRight: -2.5 },
  // A 29 pt box centers the 24 pt glyph: pull it 3.1 pt left of the inset.
  mediaIcon: {
    height: 20,
    justifyContent: "center",
    marginLeft: -3.1,
    marginRight: 2.5,
  },
  tileWrap: { flex: 1 },
  tile: {
    alignItems: "center",
    gap: 7,
    justifyContent: "center",
    minHeight: 68,
    paddingHorizontal: 8,
    paddingVertical: 12,
  },
});
