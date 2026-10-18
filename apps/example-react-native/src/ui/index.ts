// The design system. iOS-only leaves live in `<Name>.ios.tsx` next to a plain
// React Native `<Name>.tsx` with the same props (Android).
export {
  bodyScaleFor,
  useBodyScale,
  useIncreasedContrast,
  useIsAccessibilitySize,
  useReducedMotion,
  useReduceTransparency,
} from "@/ui/accessibility";
export { ActionFooter, type ActionFooterProps } from "@/ui/ActionFooter";
export { Avatar, type AvatarProps } from "@/ui/Avatar";
export {
  Bubble,
  type BubbleProps,
  BubbleText,
  ReactionBadge,
  type ReactionBadgeProps,
} from "@/ui/Bubble";
export {
  avatarPalette,
  type ColorName,
  type Palette,
  resolveColor,
  useColors,
} from "@/ui/colors";
export {
  ConnectionBanner,
  type ConnectionBannerProps,
} from "@/ui/ConnectionBanner";
export {
  ContentUnavailable,
  type ContentUnavailableProps,
} from "@/ui/ContentUnavailable";
export {
  type AlertButton,
  type ConfirmationOptions,
  showAlert,
  showConfirmation,
} from "@/ui/dialogs";
export {
  FormRow,
  type FormRowProps,
  FormScreen,
  type FormScreenProps,
  FormSection,
  type FormSectionProps,
  type RowSeparatorProps,
} from "@/ui/Form";
export {
  ButtonRow,
  type ButtonRowProps,
  Chevron,
  LabeledRow,
  type LabeledRowProps,
  NavigationRow,
  type NavigationRowProps,
  RadioRow,
  type RadioRowProps,
  TextFieldRow,
  type TextFieldRowProps,
  ToggleRow,
  type ToggleRowProps,
} from "@/ui/FormRows";
export {
  GlassButton,
  type GlassButtonProps,
  GlassIconButton,
  type GlassIconButtonProps,
} from "@/ui/GlassButton";
export { GlassSurface, type GlassSurfaceProps } from "@/ui/GlassSurface";
export { haptics } from "@/ui/haptics";
export {
  ConversationTitle,
  type ConversationTitleProps,
  HandleText,
  type HandleTextProps,
  IdentityText,
  type IdentityTextProps,
} from "@/ui/Identity";
export { normalizeHandle, splitHandle } from "@/ui/handleParts.ts";
export {
  type Motion,
  reducedTiming,
  springConfig,
  transitionScale,
  useMotion,
} from "@/ui/motion";
export {
  NavigationSubtitle,
  type NavigationSubtitleProps,
} from "@/ui/NavigationSubtitle";
export {
  PlaceholderScreen,
  type PlaceholderScreenProps,
} from "@/ui/PlaceholderScreen";
export { PlainListRow, type PlainListRowProps } from "@/ui/PlainListRow";
export {
  type SegmentedOption,
  SegmentedPicker,
  type SegmentedPickerProps,
} from "@/ui/SegmentedPicker";
export { SheetHeader, type SheetHeaderProps } from "@/ui/SheetHeader";
export {
  Symbol,
  type SymbolName,
  type SymbolProps,
  symbolNames,
} from "@/ui/Symbol";
export { Span, type SpanProps, Text, type TextProps } from "@/ui/Text";
export { avatarSizes, form, radii, sizes, spacing } from "@/ui/tokens";
export {
  type FontWeight,
  fontWeights,
  monospacedFamily,
  fontOf,
  textStyles,
  type TextVariant,
  wrappedLineHeight,
} from "@/ui/typography";
export { type ReadableStore, useStore } from "@/ui/useStore";
export { stackScreenOptions, useStackScreenOptions } from "@/ui/navigation";
export { type ToolbarIcon, useToolbarIcon } from "@/ui/toolbarIcon";
