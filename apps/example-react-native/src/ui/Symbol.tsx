import { type SymbolWeight, SymbolView } from "expo-symbols";
import { type StyleProp, type ViewStyle } from "react-native";

import { useBodyScale } from "@/ui/accessibility";
import { type ColorInput, resolveColor, useColors } from "@/ui/colors";
import { textStyles, type TextVariant } from "@/ui/typography";

/**
 * The SF Symbols the iOS example uses (`DesignSystem/Theme.swift` `Symbol`
 * and literal names), with the Material Symbol shown on Android.
 */
export const symbolNames = {
  "chevron.left": "chevron_left",
  "chevron.right": "chevron_right",
  "chevron.down": "expand_more",
  "chevron.left.forwardslash.chevron.right": "code",
  gearshape: "settings",
  "square.and.pencil": "edit_square",
  magnifyingglass: "search",
  plus: "add",
  xmark: "close",
  "arrow.up": "arrow_upward",
  "arrow.down": "arrow_downward",
  tray: "inbox",
  bell: "notifications",
  "bell.slash": "notifications_off",
  "arrowshape.turn.up.left": "reply",
  "doc.on.doc": "content_copy",
  trash: "delete",
  "arrow.clockwise": "refresh",
  photo: "image",
  doc: "description",
  "doc.fill": "draft",
  folder: "folder",
  "arrow.down.circle": "arrow_circle_down",
  "square.and.arrow.up": "ios_share",
  "square.and.arrow.down": "save_alt",
  "person.badge.plus": "person_add",
  hourglass: "hourglass_empty",
  lock: "lock",
  "lock.shield": "shield_lock",
  key: "key",
  "checkmark.seal": "verified",
  "exclamationmark.circle": "error",
  "exclamationmark.triangle": "warning",
  "exclamationmark.bubble": "sms_failed",
  nosign: "block",
  "rectangle.portrait.and.arrow.right": "logout",
  "slider.horizontal.3": "tune",
  "face.smiling": "add_reaction",
  "bubble.left.and.bubble.right": "forum",
  at: "alternate_email",
  network: "public",
  "wifi.exclamationmark": "wifi_off",
  checkmark: "check",
  "checkmark.circle.fill": "check_circle",
  circle: "radio_button_unchecked",
  "largecircle.fill.circle": "radio_button_checked",
  curlybraces: "data_object",
  "person.crop.square": "account_box",
  "person.badge.key": "admin_panel_settings",
  "person.fill.xmark": "person_remove",
  "person.crop.circle.badge.xmark": "person_cancel",
  "person.crop.circle.badge.questionmark": "contact_support",
  pencil: "edit",
} as const;

export type SymbolName = keyof typeof symbolNames;

/** SwiftUI draws a text-style symbol about 1.18 times the font size. */
const symbolBoxRatio = 1.18;

/**
 * Symbols whose SwiftUI size differs from the square default, in font-size
 * multiples: `box` sizes the glyph, `width` and `height` its layout box.
 * Measured on iOS 27: a body `doc.on.doc` row is 53.7 pt (not 52), and a
 * large-title `bubble.left.and.bubble.right` is 49 x 38 pt.
 */
const symbolMetrics: Partial<
  Record<SymbolName, { box: number; width?: number; height?: number }>
> = {
  "doc.on.doc": { box: 1.29, height: 1.39 },
  "bubble.left.and.bubble.right": { box: 1.5, width: 1.5, height: 1.17 },
};

export interface SymbolProps {
  name: SymbolName;
  /**
   * Box size in points; the symbol fits inside it. Default: the box of a
   * symbol in `textStyle` text (about 1.18 times the font size).
   */
  size?: number;
  /** Sizes the symbol like text in this style (`.font(.title3)`). Default `body`. */
  textStyle?: TextVariant;
  weight?: SymbolWeight;
  /** A palette name or any color. Default `label`. */
  color?: ColorInput;
  /** Scale with Dynamic Type, like SwiftUI symbols in text. Default true. */
  scalesWithText?: boolean;
  style?: StyleProp<ViewStyle>;
  testID?: string;
}

/**
 * An SF Symbol (iOS) or its Material Symbol (Android), sized like SwiftUI
 * `Image(systemName:)` in a text style. Decorative: hidden from accessibility.
 */
export function Symbol({
  name,
  size,
  textStyle = "body",
  weight,
  color = "label",
  scalesWithText = true,
  style,
  testID,
}: SymbolProps) {
  const palette = useColors();
  const fontScale = useBodyScale();
  const fontSize = textStyles[textStyle].fontSize;
  const metrics = size === undefined ? symbolMetrics[name] : undefined;
  const scale = scalesWithText ? fontScale : 1;
  const box = size ?? fontSize * (metrics?.box ?? symbolBoxRatio);
  const points = box * scale;
  const resolvedWeight =
    weight ?? (textStyle === "headline" ? "semibold" : undefined);
  return (
    <SymbolView
      name={{ ios: name, android: symbolNames[name] }}
      size={points}
      weight={resolvedWeight}
      tintColor={resolveColor(palette, color)}
      resizeMode="scaleAspectFit"
      style={[
        metrics === undefined
          ? undefined
          : {
              width: fontSize * (metrics.width ?? metrics.box) * scale,
              height: fontSize * (metrics.height ?? metrics.box) * scale,
            },
        style,
      ]}
      testID={testID}
      accessible={false}
      importantForAccessibility="no-hide-descendants"
      accessibilityElementsHidden
    />
  );
}
