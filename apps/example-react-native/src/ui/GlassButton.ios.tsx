import {
  Button,
  HStack,
  Host,
  ProgressView,
  Spacer,
  Text,
} from "@expo/ui/swift-ui";
import {
  accessibilityLabel as accessibilityLabelModifier,
  buttonBorderShape,
  buttonStyle,
  controlSize,
  disabled as disabledModifier,
  font,
  labelStyle,
  type ViewModifier,
} from "@expo/ui/swift-ui/modifiers";
import { StyleSheet } from "react-native";

import { useColors } from "@/ui/colors";
import type {
  GlassButtonProps,
  GlassIconButtonProps,
} from "@/ui/GlassButton.types";

export type { GlassButtonProps, GlassIconButtonProps };

/**
 * A native SwiftUI button with `.glass` or `.glassProminent` style
 * (`@expo/ui`), the iOS example's rule for every button.
 */
export function GlassButton({
  title,
  onPress,
  variant = "glass",
  role,
  size = "regular",
  font: labelFont = size === "large" ? "headline" : "body",
  systemImage,
  fullWidth = false,
  disabled = false,
  loading = false,
  testID,
  accessibilityLabel,
  style,
}: GlassButtonProps) {
  const palette = useColors();
  const modifiers: ViewModifier[] = [
    buttonStyle(variant),
    controlSize(size),
    disabledModifier(disabled),
  ];
  if (accessibilityLabel !== undefined) {
    modifiers.push(accessibilityLabelModifier(accessibilityLabel));
  }
  const label = (
    <Text modifiers={[font({ textStyle: labelFont })]}>{title}</Text>
  );
  const centered = fullWidth || loading;
  return (
    <Host
      matchContents={fullWidth ? { vertical: true } : true}
      seedColor={palette.tint}
      style={[fullWidth ? styles.fullWidth : undefined, style]}>
      <Button
        onPress={onPress}
        role={role}
        systemImage={centered ? undefined : systemImage}
        label={centered ? undefined : title}
        testID={testID}
        modifiers={
          centered ? modifiers : [...modifiers, font({ textStyle: labelFont })]
        }>
        {centered ? (
          <HStack spacing={8}>
            {fullWidth ? <Spacer /> : null}
            {loading ? <ProgressView /> : null}
            {label}
            {fullWidth ? <Spacer /> : null}
          </HStack>
        ) : undefined}
      </Button>
    </Host>
  );
}

/** An icon-only glass circle (`.labelStyle(.iconOnly)`, `.buttonBorderShape(.circle)`). */
export function GlassIconButton({
  systemImage,
  accessibilityLabel,
  onPress,
  variant = "glass",
  size = "regular",
  disabled = false,
  testID,
  style,
}: GlassIconButtonProps) {
  const palette = useColors();
  return (
    <Host matchContents seedColor={palette.tint} style={style}>
      <Button
        onPress={onPress}
        label={accessibilityLabel}
        systemImage={systemImage}
        testID={testID}
        modifiers={[
          labelStyle("iconOnly"),
          buttonBorderShape("circle"),
          buttonStyle(variant),
          controlSize(size),
          disabledModifier(disabled),
        ]}
      />
    </Host>
  );
}

const styles = StyleSheet.create({
  fullWidth: { alignSelf: "stretch" },
});
