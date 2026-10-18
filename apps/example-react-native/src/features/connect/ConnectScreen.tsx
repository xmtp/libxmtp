import { Stack } from "expo-router";
import { type ReactNode, useEffect, useState } from "react";
import {
  ActivityIndicator,
  Keyboard,
  Pressable,
  StyleSheet,
  TextInput,
  type TextInputProps,
  View,
} from "react-native";
import { KeyboardAwareScrollView } from "react-native-keyboard-controller";
import Animated, {
  LinearTransition,
  useAnimatedStyle,
  useDerivedValue,
} from "react-native-reanimated";

import { connectToAddress } from "@/core/backendUrl.ts";
import { canJoin } from "@/core/connectModel.ts";
import {
  connectModel,
  continueReset,
  deleteLocalData,
  retryRestore,
} from "@/features/connect/connectStore";
import { connectCopy } from "@/features/connect/copy";
import { Logo } from "@/features/connect/Logo";
import { useSessionState } from "@/session";
import { ActionFooter } from "@/ui/ActionFooter";
import { useColors } from "@/ui/colors";
import { showConfirmation } from "@/ui/dialogs";
import { GlassButton } from "@/ui/GlassButton";
import { useMotion } from "@/ui/motion";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { radii, sizes } from "@/ui/tokens";
import { fontOf } from "@/ui/typography";
import { useStore } from "@/ui/useStore";
import { WrappedText } from "@/ui/WrappedText";

/**
 * Join a server (`ConnectView.swift`): the hero, the server card with a
 * quiet probe status, the access token card for servers that need one, the
 * Advanced section and the Join footer. A failed restore and an unfinished
 * local reset replace the form with their recovery actions.
 */
export function ConnectScreen() {
  const palette = useColors();
  const session = useSessionState();
  const motion = useMotion();
  const model = connectModel();
  useEffect(() => {
    model.appear();
    return () => {
      model.dispose();
    };
  }, [model]);
  const state = useStore(model);
  const [advanced, setAdvanced] = useState(false);
  const recovering =
    session.kind === "resetIncomplete" || session.kind === "restoreFailed";
  const requiresToken = state.requiresToken;
  const layout = motion.reduceMotion
    ? LinearTransition.duration(200)
    : LinearTransition.springify().stiffness(503.6).damping(38.6).mass(1);

  const statusColor = state.error === undefined ? "systemGreen" : "systemRed";
  const join = () => {
    Keyboard.dismiss();
    model.join();
  };

  return (
    <View
      style={[
        styles.screen,
        { backgroundColor: palette.systemGroupedBackground },
      ]}>
      <Stack.Screen options={{ headerShown: false }} />
      <KeyboardAwareScrollView
        contentInsetAdjustmentBehavior="automatic"
        bottomOffset={footerClearance}
        // A spacer (not an inset) makes room for the keyboard, so when the
        // keyboard hides the scroll view clamps back to the top like SwiftUI.
        mode="layout"
        keyboardDismissMode="interactive"
        keyboardShouldPersistTaps="handled"
        contentContainerStyle={styles.content}>
        <View style={[styles.column, { gap: requiresToken ? 16 : 28 }]}>
          <Animated.View
            layout={layout}
            style={[styles.hero, { paddingTop: requiresToken ? 4 : 50 }]}>
            {requiresToken ? null : (
              <Animated.View
                entering={motion.entering}
                exiting={motion.exiting}
                style={styles.logo}>
                <Logo size={68} />
              </Animated.View>
            )}
            <Text
              variant="largeTitle"
              weight="bold"
              align="center"
              color={requiresToken ? "secondaryLabel" : "label"}
              style={styles.heroTitle}>
              {connectCopy.heroTitle}
            </Text>
            <Text color="themeSecondaryLabel" align="center">
              {connectCopy.heroSubtitle}
            </Text>
          </Animated.View>
          {session.kind === "resetIncomplete" ? (
            <Recovery
              text={connectCopy.resetIncomplete}
              action={connectCopy.continueReset}
              onPress={continueReset}
            />
          ) : session.kind === "restoreFailed" ? (
            <View style={styles.restoreFailed}>
              <Recovery
                text={connectCopy.restoreFailed}
                action={connectCopy.retry}
                onPress={retryRestore}
              />
              <GlassButton
                role="destructive"
                title={connectCopy.deleteLocalData}
                onPress={() => {
                  showConfirmation({
                    message: connectCopy.deleteLocalDataMessage,
                    confirm: connectCopy.deleteLocalData,
                    destructive: true,
                    onConfirm: deleteLocalData,
                  });
                }}
              />
            </View>
          ) : (
            <Animated.View layout={layout} style={styles.form}>
              <Text
                variant="footnote"
                weight="semibold"
                color="themeSecondaryLabel"
                style={styles.inset}>
                {connectCopy.yourServer}
              </Text>
              <View
                style={[
                  styles.card,
                  { backgroundColor: palette.secondarySystemGroupedBackground },
                ]}>
                <Field label={connectCopy.serverAddress}>
                  <FieldInput
                    testID="server-address"
                    accessibilityLabel={connectCopy.serverAddress}
                    value={state.address}
                    onChangeText={(text) => {
                      model.setAddress(text);
                    }}
                    placeholder={connectCopy.serverAddressPlaceholder}
                    keyboardType="url"
                    textContentType="URL"
                  />
                </Field>
                <Divider />
                <Field label={connectCopy.serverName}>
                  <FieldInput
                    accessibilityLabel={connectCopy.serverName}
                    value={state.serverName}
                    onChangeText={(text) => {
                      model.setServerName(text);
                    }}
                    placeholder={connectCopy.serverNamePlaceholder}
                  />
                </Field>
                <Divider />
                <View
                  style={[
                    styles.status,
                    { paddingVertical: requiresToken ? 12 : 16 },
                  ]}>
                  {state.checking ? (
                    <>
                      <ActivityIndicator size="small" />
                      <Text variant="subheadline" color={statusColor}>
                        {connectCopy.checking}
                      </Text>
                    </>
                  ) : (
                    <>
                      <Symbol
                        name={
                          state.error !== undefined
                            ? "exclamationmark.circle"
                            : state.ready
                              ? "checkmark.seal"
                              : "network"
                        }
                        textStyle="subheadline"
                        color={statusColor}
                      />
                      <WrappedText
                        testID="connect-status"
                        variant="subheadline"
                        color={statusColor}
                        style={styles.flex}>
                        {state.error ??
                          (state.ready
                            ? connectCopy.ready
                            : connectCopy.enterAddress)}
                      </WrappedText>
                    </>
                  )}
                  {state.checking ? <View style={styles.flex} /> : null}
                  {state.ready && requiresToken ? <TokenBadge /> : null}
                  {!state.ready &&
                  !state.checking &&
                  state.error !== undefined ? (
                    <GlassButton
                      title={connectCopy.retry}
                      font="subheadline"
                      labelColor="systemRed"
                      testID="retry-server-check"
                      onPress={() => {
                        Keyboard.dismiss();
                        model.retryProbe();
                      }}
                    />
                  ) : null}
                </View>
              </View>
              {requiresToken ? (
                <Animated.View
                  entering={motion.entering}
                  exiting={motion.exiting}
                  layout={layout}
                  style={styles.tokenSection}>
                  <View
                    style={[
                      styles.tokenCard,
                      {
                        backgroundColor:
                          palette.secondarySystemGroupedBackground,
                      },
                    ]}>
                    <View
                      style={[
                        styles.keyTile,
                        { backgroundColor: palette.tint },
                      ]}
                      accessibilityElementsHidden
                      importantForAccessibility="no-hide-descendants">
                      <Symbol name="key" color="onTint" />
                    </View>
                    <View style={styles.tokenField}>
                      <Text variant="caption" color="themeSecondaryLabel">
                        {connectCopy.accessToken}
                      </Text>
                      <FieldInput
                        accessibilityLabel={connectCopy.accessToken}
                        value={state.token}
                        onChangeText={(text) => {
                          model.setToken(text);
                        }}
                        placeholder={connectCopy.accessToken}
                        secureTextEntry
                        textContentType="password"
                      />
                    </View>
                  </View>
                  <Text
                    variant="footnote"
                    color="themeSecondaryLabel"
                    style={styles.inset}>
                    {connectCopy.tokenFootnote}
                  </Text>
                </Animated.View>
              ) : null}
              <Animated.View layout={layout}>
                <AdvancedSection
                  address={state.address}
                  expanded={advanced}
                  onToggle={() => {
                    setAdvanced((value) => !value);
                  }}
                />
              </Animated.View>
              <Animated.View layout={layout}>
                <Text
                  variant="footnote"
                  color="themeSecondaryLabel"
                  style={styles.inset}>
                  {advanced
                    ? connectCopy.advancedFootnote
                    : connectCopy.footnote}
                </Text>
              </Animated.View>
            </Animated.View>
          )}
        </View>
      </KeyboardAwareScrollView>
      {recovering ? null : (
        <ActionFooter>
          <GlassButton
            variant="glassProminent"
            size="large"
            fullWidth
            title={state.joining ? connectCopy.joining : connectCopy.join}
            loading={state.joining}
            disabled={!canJoin(state)}
            testID="join-server"
            accessibilityLabel={connectCopy.join}
            onPress={join}
          />
        </ActionFooter>
      )}
    </View>
  );
}

function Recovery({
  text,
  action,
  onPress,
}: {
  readonly text: string;
  readonly action: string;
  readonly onPress: () => void;
}) {
  return (
    <View style={styles.recovery}>
      <Text align="center">{text}</Text>
      <GlassButton variant="glassProminent" title={action} onPress={onPress} />
    </View>
  );
}

function AdvancedSection({
  address,
  expanded,
  onToggle,
}: {
  readonly address: string;
  readonly expanded: boolean;
  readonly onToggle: () => void;
}) {
  const palette = useColors();
  const motion = useMotion();
  const rotation = useDerivedValue(() => motion.animate(expanded ? 180 : 0));
  const chevron = useAnimatedStyle(() => ({
    transform: [{ rotate: `${String(rotation.value)}deg` }],
  }));
  const target = connectToAddress(address);
  return (
    <View style={styles.advanced}>
      {expanded ? (
        <Animated.View entering={motion.entering} exiting={motion.exiting}>
          <Text
            variant="footnote"
            weight="semibold"
            color="themeSecondaryLabel"
            style={styles.inset}>
            {connectCopy.advanced}
          </Text>
        </Animated.View>
      ) : null}
      <View
        style={[
          styles.card,
          { backgroundColor: palette.secondarySystemGroupedBackground },
        ]}>
        {expanded ? (
          <Animated.View entering={motion.entering} exiting={motion.exiting}>
            <Field label={connectCopy.connectTo}>
              {target === undefined ? (
                <Text color="themeSecondaryLabel">
                  {connectCopy.enterAddress}
                </Text>
              ) : (
                <Text monospaced selectable>
                  {target}
                </Text>
              )}
            </Field>
            <Divider />
          </Animated.View>
        ) : null}
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={
            expanded
              ? connectCopy.hideAdvancedSettings
              : connectCopy.advancedSettings
          }
          accessibilityValue={{
            text: expanded ? connectCopy.expanded : connectCopy.collapsed,
          }}
          onPress={onToggle}
          style={({ pressed }) => [
            styles.toggle,
            pressed ? styles.pressed : undefined,
          ]}>
          <Text color="accentText" style={styles.flex}>
            {expanded
              ? connectCopy.hideAdvancedSettings
              : connectCopy.advancedSettings}
          </Text>
          <Animated.View style={chevron}>
            <Symbol name="chevron.down" color="accentText" />
          </Animated.View>
        </Pressable>
      </View>
    </View>
  );
}

function TokenBadge() {
  const palette = useColors();
  return (
    <View
      style={[styles.badge, { backgroundColor: palette.tertiarySystemFill }]}
      accessible
      accessibilityLabel={connectCopy.needsToken}>
      <Symbol
        name="lock"
        textStyle="caption"
        weight="semibold"
        color="secondaryLabel"
      />
      <Text variant="caption" weight="semibold" color="secondaryLabel">
        {connectCopy.needsToken}
      </Text>
    </View>
  );
}

function Field({
  label,
  children,
}: {
  readonly label: string;
  readonly children: ReactNode;
}) {
  return (
    <View style={styles.field}>
      <Text variant="caption" color="themeSecondaryLabel">
        {label}
      </Text>
      {children}
    </View>
  );
}

/** A plain `TextField`: body text, no capitalization or autocorrection. */
function FieldInput(props: TextInputProps) {
  const palette = useColors();
  return (
    <TextInput
      autoCapitalize="none"
      autoCorrect={false}
      spellCheck={false}
      placeholderTextColor={palette.placeholderText}
      selectionColor={palette.tint}
      {...props}
      style={[fontOf("body"), styles.input, { color: palette.label }]}
    />
  );
}

function Divider() {
  const palette = useColors();
  return (
    <View style={[styles.divider, { backgroundColor: palette.divider }]} />
  );
}

/**
 * Space kept between a focused field and the keyboard: the footer rides above
 * the keyboard (`ActionFooter`), so the field stays above the footer too.
 */
const footerClearance = 90;

const styles = StyleSheet.create({
  screen: { flex: 1 },
  content: { alignItems: "center", paddingBottom: 24, paddingHorizontal: 16 },
  column: { maxWidth: sizes.readableWidth, width: "100%" },
  hero: { alignItems: "center", gap: 12 },
  logo: { marginBottom: 6 },
  heroTitle: { fontSize: 32 },
  form: { gap: 12 },
  inset: { paddingHorizontal: 16 },
  card: {
    borderCurve: "continuous",
    borderRadius: radii.card,
    overflow: "hidden",
  },
  field: { gap: 4, paddingHorizontal: 16, paddingVertical: 10 },
  // Android's TextInput adds its own padding. SwiftUI's TextField line is
  // about 2 pt taller than React Native's, with the text 1 pt lower.
  input: { margin: 0, paddingBottom: 0.7, paddingHorizontal: 0, paddingTop: 1 },
  divider: { height: StyleSheet.hairlineWidth, marginLeft: 16 },
  status: {
    alignItems: "center",
    flexDirection: "row",
    gap: 10,
    minHeight: 52,
    paddingHorizontal: 16,
  },
  flex: { flex: 1 },
  badge: {
    alignItems: "center",
    borderRadius: radii.capsule,
    flexDirection: "row",
    gap: 4,
    paddingHorizontal: 12,
    paddingVertical: 6,
  },
  tokenSection: { gap: 8, paddingTop: 4 },
  tokenCard: {
    alignItems: "center",
    borderCurve: "continuous",
    borderRadius: radii.card,
    flexDirection: "row",
    gap: 12,
    paddingHorizontal: 16,
    paddingVertical: 12,
  },
  keyTile: {
    alignItems: "center",
    borderCurve: "continuous",
    borderRadius: 8,
    height: 30,
    justifyContent: "center",
    width: 30,
  },
  tokenField: { flex: 1, gap: 4 },
  advanced: { gap: 8 },
  toggle: {
    alignItems: "center",
    flexDirection: "row",
    // `.frame(minHeight: 44)` inside `.padding(.vertical, 4)`.
    minHeight: sizes.minimumHitTarget + 8,
    paddingHorizontal: 16,
    paddingVertical: 4,
  },
  pressed: { opacity: 0.6 },
  recovery: { alignItems: "center", gap: 20, padding: 24 },
  restoreFailed: { alignItems: "center", gap: 16 },
});
