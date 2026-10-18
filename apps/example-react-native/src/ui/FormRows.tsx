import type { ComponentRef, Ref } from "react";
import {
  StyleSheet,
  Switch,
  TextInput,
  type TextInputProps,
  View,
} from "react-native";

import { type ColorInput, resolveColor, useColors } from "@/ui/colors";
import { FormRow, type RowSeparatorProps } from "@/ui/Form";
import { Symbol, type SymbolName } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { form } from "@/ui/tokens";
import { fontOf } from "@/ui/typography";

const iconColumnInset = form.rowInset + form.iconColumn + form.iconGap;
/** Settings `actionRow` icons sit in `.frame(width: 20)`. */
const actionIconWidth = 20;
/** A SwiftUI `Label` in a list row gives its icon a 28 pt column. */
const labelIconWidth = 28;
const tileInset = form.rowInset + form.iconTileSize + form.iconGap;

/** `chevron.right` in `.caption.weight(.semibold)` at the trailing edge. */
export function Chevron({ color = "tertiaryLabel" }: { color?: ColorInput }) {
  return (
    <Symbol
      name="chevron.right"
      textStyle="caption"
      weight="semibold"
      color={color}
    />
  );
}

interface PressableRowProps extends RowSeparatorProps {
  onPress?: () => void;
  disabled?: boolean;
  testID?: string;
  accessibilityLabel?: string;
  accessibilityHint?: string;
}

export interface NavigationRowProps extends PressableRowProps {
  title: string;
  systemImage?: SymbolName;
  /**
   * `plain`: a symbol in a 24 pt column (Details rows, system blue).
   * `tile`: a white symbol on a 28 pt rounded tile (Settings rows).
   */
  icon?: "plain" | "tile";
  /** Symbol color (`plain`) or tile fill (`tile`). */
  iconColor?: ColorInput;
  /** Trailing secondary value before the chevron. */
  value?: string;
}

/** A row that pushes a screen: icon, title, optional value, chevron. */
export function NavigationRow({
  title,
  systemImage,
  icon = "plain",
  iconColor,
  value,
  onPress,
  disabled,
  testID,
  accessibilityLabel,
  accessibilityHint,
}: NavigationRowProps) {
  const palette = useColors();
  return (
    <FormRow
      onPress={onPress}
      disabled={disabled}
      testID={testID}
      accessibilityLabel={
        accessibilityLabel ??
        (value === undefined ? title : `${title}, ${value}`)
      }
      accessibilityHint={accessibilityHint}>
      {systemImage === undefined ? null : icon === "tile" ? (
        <View
          style={[
            styles.tile,
            { backgroundColor: resolveColor(palette, iconColor ?? "tint") },
          ]}>
          <Symbol name={systemImage} size={17} color="onTint" />
        </View>
      ) : (
        <View style={styles.iconColumn}>
          <Symbol name={systemImage} color={iconColor ?? "systemBlue"} />
        </View>
      )}
      <Text style={styles.flexText}>{title}</Text>
      {value === undefined ? null : (
        <Text color="secondaryLabel" numberOfLines={1}>
          {value}
        </Text>
      )}
      <Chevron />
    </FormRow>
  );
}
NavigationRow.separatorInset = (props: NavigationRowProps): number =>
  props.systemImage === undefined
    ? form.rowInset
    : props.icon === "tile"
      ? tileInset
      : iconColumnInset;

export interface ButtonRowProps extends PressableRowProps {
  title: string;
  systemImage?: SymbolName;
  /** `destructive` draws in system red. */
  role?: "destructive";
  /** Text and icon color. Default `tint` (`systemRed` when destructive). */
  color?: ColorInput;
  /** Icon color when it differs from the text (Details "Add member"). */
  iconColor?: ColorInput;
  /** Icon box in points (a list `Label` draws its symbol larger: about 24). */
  iconSize?: number;
  /** A trailing chevron in the row color at 25 percent (Settings "Change handle"). */
  navigates?: boolean;
  /**
   * `action` (default): the Settings `actionRow`, a 20 pt icon column.
   * `label`: a SwiftUI `Label` row (Details "Add member"), a 28 pt column.
   */
  iconStyle?: "action" | "label";
}

/**
 * A tinted action row (Settings `actionRow`): an icon column and the title in
 * the tint, or system red when destructive.
 */
export function ButtonRow({
  title,
  systemImage,
  role,
  color,
  navigates = false,
  iconStyle = "action",
  iconColor,
  iconSize,
  onPress,
  disabled,
  testID,
  accessibilityLabel,
  accessibilityHint,
}: ButtonRowProps) {
  const tint = color ?? (role === "destructive" ? "systemRed" : "tint");
  const iconWidth = iconStyle === "label" ? labelIconWidth : actionIconWidth;
  return (
    <FormRow
      onPress={onPress}
      disabled={disabled}
      testID={testID}
      accessibilityLabel={accessibilityLabel ?? title}
      accessibilityHint={accessibilityHint}>
      {systemImage === undefined ? null : (
        <View style={[styles.actionIcon, { width: iconWidth }]}>
          <Symbol
            name={systemImage}
            color={iconColor ?? tint}
            size={iconSize}
          />
        </View>
      )}
      <Text color={tint} style={styles.flexText}>
        {title}
      </Text>
      {navigates ? (
        <Chevron color={tint === "tint" ? "tintTertiary" : "tertiaryLabel"} />
      ) : null}
    </FormRow>
  );
}
ButtonRow.separatorInset = (props: ButtonRowProps): number =>
  props.systemImage === undefined
    ? form.rowInset
    : form.rowInset +
      (props.iconStyle === "label" ? labelIconWidth : actionIconWidth) +
      form.iconGap;

export interface LabeledRowProps extends RowSeparatorProps {
  label: string;
  value: string;
  /**
   * `inline`: `LabeledContent` (label left, secondary value right).
   * `stacked`: a caption label over the value (Settings "Inbox ID", "Address").
   */
  layout?: "inline" | "stacked";
  /** Value font in the stacked layout. Default `body`. */
  valueVariant?: "body" | "footnote";
  monospaced?: boolean;
  /** Truncate the stacked value to one line at this position. */
  truncate?: "tail" | "middle";
  selectable?: boolean;
  testID?: string;
  accessibilityLabel?: string;
}

/** A read-only label and value. */
export function LabeledRow({
  label,
  value,
  layout = "inline",
  valueVariant = "body",
  monospaced = false,
  truncate,
  selectable = false,
  testID,
  accessibilityLabel,
}: LabeledRowProps) {
  if (layout === "stacked") {
    return (
      <FormRow
        testID={testID}
        accessibilityLabel={accessibilityLabel ?? label}
        accessibilityValue={{ text: value }}
        style={styles.stacked}>
        <Text variant="caption" color="themeSecondaryLabel">
          {label}
        </Text>
        <Text
          variant={valueVariant}
          monospaced={monospaced}
          selectable={selectable}
          numberOfLines={truncate === undefined ? undefined : 1}
          ellipsizeMode={truncate}>
          {value}
        </Text>
      </FormRow>
    );
  }
  return (
    <FormRow
      testID={testID}
      accessibilityLabel={accessibilityLabel ?? label}
      accessibilityValue={{ text: value }}>
      <Text>{label}</Text>
      <Text
        color="secondaryLabel"
        monospaced={monospaced}
        selectable={selectable}
        align="right"
        style={styles.flexText}>
        {value}
      </Text>
    </FormRow>
  );
}

export interface ToggleRowProps extends RowSeparatorProps {
  title: string;
  value: boolean;
  onValueChange: (value: boolean) => void;
  disabled?: boolean;
  testID?: string;
}

/** A SwiftUI `Toggle` row: the system switch, on color in the tint. */
export function ToggleRow({
  title,
  value,
  onValueChange,
  disabled = false,
  testID,
}: ToggleRowProps) {
  const palette = useColors();
  return (
    <FormRow style={styles.toggleRow}>
      <Text style={styles.flexText}>{title}</Text>
      <Switch
        testID={testID}
        accessibilityLabel={title}
        value={value}
        onValueChange={onValueChange}
        disabled={disabled}
        trackColor={{ true: palette.tint }}
      />
    </FormRow>
  );
}

export interface TextFieldRowProps extends TextInputProps, RowSeparatorProps {
  ref?: Ref<ComponentRef<typeof TextInput>>;
}

/** A `TextField` row in body text with the system placeholder color. */
export function TextFieldRow({ style, ref, ...props }: TextFieldRowProps) {
  const palette = useColors();
  return (
    <FormRow style={styles.fieldRow}>
      <TextInput
        ref={ref}
        placeholderTextColor={palette.placeholderText}
        selectionColor={palette.tint}
        {...props}
        style={[fontOf("body"), styles.field, { color: palette.label }, style]}
      />
    </FormRow>
  );
}

export interface RadioRowProps extends PressableRowProps {
  title: string;
  /** Footnote under the title (Permissions presets). */
  detail?: string;
  selected: boolean;
  /**
   * `radio`: a leading `largecircle.fill.circle` / `circle` (Permissions).
   * `checkmark`: a trailing `checkmark` with the title in the tint
   * (Disappearing presets, a default-style List button).
   */
  indicator?: "radio" | "checkmark";
  /** The spoken label of the checkmark (`"Selected"`). */
  selectedLabel?: string;
}

/** One choice in a single-selection list. */
export function RadioRow({
  title,
  detail,
  selected,
  indicator = "radio",
  selectedLabel,
  onPress,
  disabled,
  testID,
  accessibilityLabel,
  accessibilityHint,
}: RadioRowProps) {
  const accessibility = {
    testID,
    accessibilityLabel:
      accessibilityLabel ??
      (detail === undefined ? title : `${title}, ${detail}`),
    accessibilityHint,
    accessibilityState: { selected },
    accessibilityValue:
      selected && selectedLabel !== undefined
        ? { text: selectedLabel }
        : undefined,
  };
  if (indicator === "checkmark") {
    return (
      <FormRow onPress={onPress} disabled={disabled} {...accessibility}>
        <Text color="tint" style={styles.flexText}>
          {title}
        </Text>
        {selected ? <Symbol name="checkmark" color="tint" /> : null}
      </FormRow>
    );
  }
  return (
    <FormRow onPress={onPress} disabled={disabled} {...accessibility}>
      <View style={styles.iconColumn}>
        <Symbol
          name={selected ? "largecircle.fill.circle" : "circle"}
          textStyle="title3"
          color={selected ? "tint" : "themeSecondaryLabel"}
        />
      </View>
      <View style={styles.radioText}>
        <Text>{title}</Text>
        {detail === undefined ? null : (
          <Text variant="footnote" color="themeSecondaryLabel">
            {detail}
          </Text>
        )}
      </View>
    </FormRow>
  );
}
RadioRow.separatorInset = (props: RadioRowProps): number =>
  props.indicator === "checkmark" ? form.rowInset : iconColumnInset;

const styles = StyleSheet.create({
  flexText: { flex: 1 },
  iconColumn: { alignItems: "center", minWidth: form.iconColumn },
  actionIcon: { alignItems: "center" },
  tile: {
    alignItems: "center",
    borderCurve: "continuous",
    borderRadius: 7,
    height: form.iconTileSize,
    justifyContent: "center",
    width: form.iconTileSize,
  },
  stacked: { alignItems: "flex-start", flexDirection: "column", gap: 5 },
  toggleRow: { paddingVertical: 10 },
  fieldRow: { paddingVertical: 0 },
  field: { flex: 1, minHeight: form.rowMinHeight, paddingVertical: 0 },
  radioText: { flex: 1, gap: 4 },
});
