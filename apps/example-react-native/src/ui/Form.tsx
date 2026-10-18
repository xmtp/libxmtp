import { Stack } from "expo-router";
import {
  Children,
  createContext,
  isValidElement,
  type ReactElement,
  type ReactNode,
  use,
} from "react";
import {
  Pressable,
  ScrollView,
  type ScrollViewProps,
  type StyleProp,
  StyleSheet,
  View,
  type ViewStyle,
} from "react-native";

import { useColors } from "@/ui/colors";
import { Text } from "@/ui/Text";
import { form } from "@/ui/tokens";
import { WrappedText } from "@/ui/WrappedText";

/**
 * An iOS 26 inset-grouped list (`Form`, `List(.insetGrouped)`) drawn in React
 * Native. Metrics are measured against SwiftUI `Form` on iOS 27 (402 pt
 * iPhone): sections 16 pt from the screen edge with a 26 pt continuous radius,
 * rows at least 52 pt tall with 15 pt vertical padding, 1 pt separators that
 * start at the row text, 17 pt semibold headers and footnote footers.
 */

/**
 * Space a section adds above itself. Measured on iOS 27 (card or footer
 * bottom to the next header line or card): card to header 27.4, footer to
 * header 16.0, card to card 35.0, footer to card 24.1. The first section
 * starts right under the large title, or its header line 8.7 pt lower.
 */
const sectionTop = {
  header: 16.4,
  noHeader: 23.9,
  firstHeader: 8.7,
  firstNoHeader: 0,
} as const;
/** Space below a section that has no footer. */
const sectionBottomWithoutFooter = 11;
/** Header line to the card. */
const headerGap = 10.3;
/** Card to the footer line. */
const footerTop = 7.9;

/**
 * Children of a route that render no view. `FormScreen` skips them when it
 * looks for the first section.
 */
const nonVisual = new Set<unknown>([
  Stack.Screen,
  Stack.Toolbar,
  Stack.SearchBar,
  Stack.Title,
]);

function isSectionCandidate(child: ReactNode): boolean {
  return !isValidElement(child) || !nonVisual.has(child.type);
}

const FirstSectionContext = createContext(false);

export interface FormScreenProps extends ScrollViewProps {
  children?: ReactNode;
  /**
   * Extra space after the last section. Default 0: SwiftUI ends the content
   * right after the last footer (above the home indicator inset).
   */
  bottomInset?: number;
}

/**
 * The scrolling grouped screen. Make it the route's first view so the native
 * header finds it: the large title collapses and the scroll edge effect runs.
 * `Stack.Screen`, `Stack.Toolbar`, `Stack.SearchBar` and `Stack.Title` may sit
 * inside it; the first other child is the first section.
 */
export function FormScreen({
  children,
  bottomInset = 0,
  style,
  contentContainerStyle,
  ...props
}: FormScreenProps) {
  const palette = useColors();
  const sections = Children.toArray(children);
  const firstIndex = sections.findIndex(isSectionCandidate);
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      keyboardDismissMode="interactive"
      keyboardShouldPersistTaps="handled"
      {...props}
      style={[{ backgroundColor: palette.systemGroupedBackground }, style]}
      contentContainerStyle={[
        { paddingBottom: bottomInset },
        contentContainerStyle,
      ]}>
      {sections.map((section, index) => (
        <FirstSectionContext
          key={
            isValidElement(section) && section.key !== null
              ? section.key
              : index
          }
          value={index === firstIndex}>
          {section}
        </FirstSectionContext>
      ))}
    </ScrollView>
  );
}

/** Props every row accepts so `FormSection` can draw its separator. */
export interface RowSeparatorProps {
  /**
   * Where the separator below this row starts, from the section edge. SwiftUI
   * aligns it with the row's first text. Default 16 (plain text rows).
   */
  separatorInset?: number;
}

export interface FormSectionProps {
  /** Sentence-case header (`Section { } header: { Text(...) }`). */
  header?: string;
  /** Trailing header content (New message "Saved on this device"). */
  headerTrailing?: ReactNode;
  /** Footnote text under the section. */
  footer?: ReactNode;
  children?: ReactNode;
  /** Draws the rows without the white card (`listRowBackground(.clear)`). */
  plain?: boolean;
  style?: StyleProp<ViewStyle>;
  testID?: string;
}

function separatorInsetOf(child: ReactNode): number {
  if (!isValidElement(child)) return form.rowInset;
  const props = child.props as RowSeparatorProps;
  if (typeof props.separatorInset === "number") return props.separatorInset;
  const type = child.type as { separatorInset?: (props: unknown) => number };
  return typeof type.separatorInset === "function"
    ? type.separatorInset(child.props)
    : form.rowInset;
}

/** One inset-grouped section: header, rows with separators, footer. */
export function FormSection({
  header,
  headerTrailing,
  footer,
  children,
  plain = false,
  style,
  testID,
}: FormSectionProps) {
  const palette = useColors();
  const first = use(FirstSectionContext);
  const rows = Children.toArray(children);
  const top =
    header === undefined
      ? first
        ? sectionTop.firstNoHeader
        : sectionTop.noHeader
      : first
        ? sectionTop.firstHeader
        : sectionTop.header;
  return (
    <View
      testID={testID}
      style={[
        styles.section,
        {
          paddingTop: top,
          paddingBottom: footer === undefined ? sectionBottomWithoutFooter : 0,
        },
        style,
      ]}>
      {header === undefined ? null : headerTrailing === undefined ? (
        <Text
          variant="headline"
          color="secondaryLabel"
          accessibilityRole="header"
          style={styles.header}>
          {header}
        </Text>
      ) : (
        <View style={[styles.header, styles.headerRow]}>
          <Text
            variant="headline"
            color="secondaryLabel"
            accessibilityRole="header"
            style={styles.headerTitle}>
            {header}
          </Text>
          {headerTrailing}
        </View>
      )}
      <View
        style={[
          plain
            ? undefined
            : [
                styles.card,
                { backgroundColor: palette.secondarySystemGroupedBackground },
              ],
        ]}>
        {rows.map((row, index) => (
          <View key={isValidElement(row) && row.key !== null ? row.key : index}>
            {row}
            {index < rows.length - 1 && !plain ? (
              <View
                pointerEvents="none"
                style={[
                  styles.separator,
                  {
                    backgroundColor: palette.separator,
                    left: separatorInsetOf(row as ReactElement),
                  },
                ]}
              />
            ) : null}
          </View>
        ))}
      </View>
      {footer === undefined ? null : typeof footer === "string" ? (
        <WrappedText
          variant="footnote"
          color="secondaryLabel"
          style={styles.footer}>
          {footer}
        </WrappedText>
      ) : (
        <View style={styles.footer}>{footer}</View>
      )}
    </View>
  );
}

export interface FormRowProps extends RowSeparatorProps {
  children?: ReactNode;
  /** Makes the row a button with the system highlight. */
  onPress?: () => void;
  onLongPress?: () => void;
  disabled?: boolean;
  /** Row padding override (Settings' segmented picker uses 8). */
  padding?: number;
  testID?: string;
  accessibilityLabel?: string;
  accessibilityHint?: string;
  accessibilityRole?:
    | "button"
    | "link"
    | "text"
    | "header"
    | "switch"
    | "radio";
  accessibilityState?: {
    selected?: boolean;
    checked?: boolean;
    disabled?: boolean;
  };
  accessibilityValue?: { text?: string };
  style?: StyleProp<ViewStyle>;
}

/** The base row: 52 pt minimum, 16 pt sides, 15 pt top and bottom. */
export function FormRow({
  children,
  onPress,
  onLongPress,
  disabled = false,
  padding,
  testID,
  accessibilityLabel,
  accessibilityHint,
  accessibilityRole,
  accessibilityState,
  accessibilityValue,
  style,
}: FormRowProps) {
  const palette = useColors();
  const layout = [
    styles.row,
    padding === undefined
      ? undefined
      : { paddingHorizontal: padding, paddingVertical: padding },
    style,
  ];
  if (onPress === undefined && onLongPress === undefined) {
    return (
      <View
        testID={testID}
        accessible={accessibilityLabel !== undefined}
        accessibilityLabel={accessibilityLabel}
        accessibilityHint={accessibilityHint}
        accessibilityRole={accessibilityRole}
        accessibilityState={accessibilityState}
        accessibilityValue={accessibilityValue}
        style={layout}>
        {children}
      </View>
    );
  }
  return (
    <Pressable
      testID={testID}
      onPress={onPress}
      onLongPress={onLongPress}
      disabled={disabled}
      accessibilityLabel={accessibilityLabel}
      accessibilityHint={accessibilityHint}
      accessibilityRole={accessibilityRole ?? "button"}
      accessibilityState={{ disabled, ...accessibilityState }}
      accessibilityValue={accessibilityValue}
      unstable_pressDelay={60}
      style={({ pressed }) => [
        layout,
        pressed ? { backgroundColor: palette.rowHighlight } : undefined,
        disabled ? styles.disabled : undefined,
      ]}>
      {children}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  section: { paddingHorizontal: form.margin },
  header: { marginBottom: headerGap, marginHorizontal: form.rowInset },
  headerRow: { alignItems: "center", flexDirection: "row", gap: 8 },
  headerTitle: { flex: 1 },
  card: {
    borderCurve: "continuous",
    borderRadius: form.sectionRadius,
    overflow: "hidden",
  },
  separator: {
    bottom: 0,
    height: 1,
    position: "absolute",
    right: form.rowInset,
  },
  footer: { marginHorizontal: form.rowInset, marginTop: footerTop },
  row: {
    alignItems: "center",
    flexDirection: "row",
    gap: form.iconGap,
    minHeight: form.rowMinHeight,
    paddingHorizontal: form.rowInset,
    paddingVertical: form.rowPaddingVertical,
  },
  disabled: { opacity: 0.4 },
});
