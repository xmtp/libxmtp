import { Stack } from "expo-router";
import { type ReactNode, useEffect, useState } from "react";
import { ScrollView, StyleSheet, TextInput, View } from "react-native";

import { canJoin, ConnectStore } from "@/features/connect/connectStore";
import { connectCopy } from "@/features/connect/copy";
import { launchEnvironment } from "@/platform/messengerPlatform";
import { useSessionState } from "@/session";
import { ActionFooter } from "@/ui/ActionFooter";
import { useColors } from "@/ui/colors";
import { showConfirmation } from "@/ui/dialogs";
import { GlassButton } from "@/ui/GlassButton";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { sizes } from "@/ui/tokens";
import { fontOf } from "@/ui/typography";
import { useStore } from "@/ui/useStore";

/** `ConnectState.address` default when the launch environment has none. */
const DEFAULT_BACKEND_URL = "http://127.0.0.1:5050";

/**
 * A basic Connect screen (`ConnectView.swift`): the hero, the server address
 * and name fields, the Join footer, and the recovery blocks for a failed
 * restore and an unfinished local reset. The account lane adds the quiet
 * probe, the token card and the advanced section.
 */
export function ConnectScreen() {
  const palette = useColors();
  const session = useSessionState();
  const [store] = useState(
    () =>
      new ConnectStore(launchEnvironment().backendUrl ?? DEFAULT_BACKEND_URL),
  );
  useEffect(() => () => store.dispose(), [store]);
  const state = useStore(store);
  const recovering =
    session.kind === "resetIncomplete" || session.kind === "restoreFailed";

  return (
    <View
      style={[
        styles.screen,
        { backgroundColor: palette.systemGroupedBackground },
      ]}>
      <Stack.Screen options={{ headerShown: false }} />
      <ScrollView
        contentInsetAdjustmentBehavior="automatic"
        keyboardDismissMode="interactive"
        keyboardShouldPersistTaps="handled"
        contentContainerStyle={styles.content}>
        <View style={styles.column}>
          <View style={styles.hero}>
            <Text
              variant="largeTitle"
              weight="bold"
              align="center"
              style={styles.heroTitle}>
              {connectCopy.heroTitle}
            </Text>
            <Text color="themeSecondaryLabel" align="center">
              {connectCopy.heroSubtitle}
            </Text>
          </View>
          {session.kind === "resetIncomplete" ? (
            <View style={styles.recovery}>
              <Text variant="headline" align="center">
                {connectCopy.resetIncomplete}
              </Text>
              <GlassButton
                variant="glassProminent"
                title={connectCopy.continueReset}
                onPress={() => void store.continueReset()}
              />
              <ErrorText error={state.error} />
            </View>
          ) : session.kind === "restoreFailed" ? (
            <View style={styles.recovery}>
              <Text variant="headline" align="center">
                {connectCopy.restoreFailed}
              </Text>
              <GlassButton
                variant="glassProminent"
                title={connectCopy.retry}
                onPress={() => void store.retryRestore()}
              />
              <GlassButton
                role="destructive"
                title={connectCopy.deleteLocalData}
                onPress={() => {
                  showConfirmation({
                    title: connectCopy.deleteLocalDataTitle,
                    message: connectCopy.deleteLocalDataMessage,
                    confirm: connectCopy.deleteLocalData,
                    destructive: true,
                    onConfirm: () => void store.deleteLocalData(),
                  });
                }}
              />
              <ErrorText error={state.error} />
            </View>
          ) : (
            <View style={styles.form}>
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
                  <TextInput
                    testID="server-address"
                    value={state.address}
                    onChangeText={(text) => {
                      store.setAddress(text);
                    }}
                    placeholder={connectCopy.serverAddressPlaceholder}
                    placeholderTextColor={palette.placeholderText}
                    selectionColor={palette.tint}
                    keyboardType="url"
                    autoCapitalize="none"
                    autoCorrect={false}
                    editable={!state.joining}
                    style={[fontOf("body"), { color: palette.label }]}
                  />
                </Field>
                <Divider />
                <Field label={connectCopy.serverName}>
                  <TextInput
                    value={state.name}
                    onChangeText={(text) => {
                      store.setName(text);
                    }}
                    placeholder={connectCopy.serverNamePlaceholder}
                    placeholderTextColor={palette.placeholderText}
                    selectionColor={palette.tint}
                    editable={!state.joining}
                    style={[fontOf("body"), { color: palette.label }]}
                  />
                </Field>
                <Divider />
                <View style={styles.status}>
                  <Symbol
                    name={
                      state.error === undefined
                        ? "network"
                        : "exclamationmark.circle"
                    }
                    textStyle="subheadline"
                    color={
                      state.error === undefined ? "systemGreen" : "systemRed"
                    }
                  />
                  <Text
                    testID="connect-status"
                    variant="subheadline"
                    color={
                      state.error === undefined ? "systemGreen" : "systemRed"
                    }
                    style={styles.flex}>
                    {state.error ?? connectCopy.enterAddress}
                  </Text>
                </View>
              </View>
              <Text
                variant="footnote"
                color="themeSecondaryLabel"
                style={styles.inset}>
                {connectCopy.footnote}
              </Text>
            </View>
          )}
        </View>
      </ScrollView>
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
            onPress={() => void store.join()}
          />
        </ActionFooter>
      )}
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

function Divider() {
  const palette = useColors();
  return (
    <View style={[styles.divider, { backgroundColor: palette.separator }]} />
  );
}

function ErrorText({ error }: { readonly error: string | undefined }) {
  if (error === undefined) return null;
  return (
    <Text
      testID="connect-status"
      variant="subheadline"
      color="systemRed"
      align="center">
      {error}
    </Text>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  content: { alignItems: "center", paddingBottom: 24, paddingHorizontal: 16 },
  column: { gap: 28, maxWidth: sizes.readableWidth, width: "100%" },
  hero: { gap: 12, paddingTop: 50 },
  heroTitle: { fontSize: 32 },
  form: { gap: 12 },
  inset: { paddingHorizontal: 16 },
  card: { borderCurve: "continuous", borderRadius: 24, overflow: "hidden" },
  field: { gap: 4, paddingHorizontal: 16, paddingVertical: 10 },
  divider: { height: StyleSheet.hairlineWidth, marginLeft: 16 },
  status: {
    alignItems: "center",
    flexDirection: "row",
    gap: 10,
    paddingHorizontal: 16,
    paddingVertical: 16,
  },
  flex: { flex: 1 },
  recovery: { alignItems: "center", gap: 16, padding: 24 },
});
