import {
  Button,
  HStack,
  Host,
  Image,
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
  foregroundStyle,
  frame,
  labelStyle,
  type ViewModifier,
} from "@expo/ui/swift-ui/modifiers";
import { StyleSheet } from "react-native";

import { resolveColor, useColors } from "@/ui/colors";
import type {
  GlassButtonProps,
  GlassIconButtonProps,
} from "@/ui/GlassButton.types";

export type { GlassButtonProps, GlassIconButtonProps };

/**
 * A native SwiftUI button with `.glass` or `.glassProminent` style
 * (`@expo/ui`). Every button in the app uses it.
 */
export function GlassButton({
  title,
  onPress,
  variant = "glass",
  role,
  size = "regular",
  font: labelFont = size === "large" ? "headline" : "body",
  systemImage,
  trailingSystemImage,
  labelFills = false,
  fullWidth = false,
  disabled = false,
  loading = false,
  labelColor,
  testID,
  accessibilityLabel,
  style,
}: GlassButtonProps) {
  const palette = useColors();
  const trailing = fullWidth ? trailingSystemImage : undefined;
  const modifiers: ViewModifier[] = [
    buttonStyle(variant),
    controlSize(size),
    disabledModifier(disabled),
  ];
  const label = resolveColor(palette, labelColor);
  if (label !== undefined) modifiers.push(foregroundStyle(label));
  if (accessibilityLabel !== undefined) {
    modifiers.push(accessibilityLabelModifier(accessibilityLabel));
  }
  const text = (
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
          fullWidth && labelFills && trailing === undefined && !loading ? (
            <Text
              modifiers={[
                font({ textStyle: labelFont }),
                frame({ maxWidth: 100_000 }),
              ]}>
              {title}
            </Text>
          ) : trailing === undefined ? (
            <HStack spacing={8}>
              {fullWidth ? <Spacer /> : null}
              {loading ? <ProgressView /> : null}
              {text}
              {fullWidth ? <Spacer /> : null}
            </HStack>
          ) : (
            <HStack spacing={8}>
              {text}
              <Spacer />
              <Image
                systemName={trailing}
                modifiers={[font({ textStyle: labelFont })]}
              />
            </HStack>
          )
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
  // Fills the width in a row or a column parent.
  fullWidth: { width: "100%" },
});
