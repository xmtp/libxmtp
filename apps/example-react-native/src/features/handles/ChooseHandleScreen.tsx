import { Stack } from "expo-router";
import {
  ActivityIndicator,
  Pressable,
  ScrollView,
  StyleSheet,
  TextInput,
  View,
} from "react-native";

import {
  type HandleModel,
  type HandleState,
  canRetryNotice,
} from "@/core/handleModel.ts";
import {
  canCheck,
  canClaim,
  handleHint,
  handleNoticeCopy,
  handleProblemCopy,
  hasMoreSuggestions,
  isEditable,
  primaryButtonTitle,
  statusPick,
  visibleSuggestions,
} from "@/core/handles.ts";
import { handleCopy } from "@/features/handles/copy";
import {
  useHandleModel,
  useLargeTitleScale,
} from "@/features/handles/useHandleModel";
import { type ActiveSession, useActiveSession } from "@/session";
import { ActionFooter } from "@/ui/ActionFooter";
import { useColors } from "@/ui/colors";
import { GlassButton } from "@/ui/GlassButton";
import { HandleText } from "@/ui/Identity";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { radii, sizes } from "@/ui/tokens";
import { fontOf } from "@/ui/typography";
import { WrappedText } from "@/ui/WrappedText";

/**
 * Choose a handle after the first join (`HandleView.swift`, mode `.choose`):
 * no navigation bar, a hero, the capsule field, availability feedback, and
 * the claim and "Skip for now" footer.
 */
export function ChooseHandleScreen() {
  const session = useActiveSession();
  if (session === undefined) return null;
  return <ChooseHandle key={session.generation} session={session} />;
}

function ChooseHandle({ session }: { readonly session: ActiveSession }) {
  const palette = useColors();
  const { model, state } = useHandleModel(session, "choose");
  const heroSize = 72 * useLargeTitleScale();
  const editable = isEditable(state);
  return (
    <View
      style={[
        styles.screen,
        { backgroundColor: palette.systemGroupedBackground },
      ]}>
      <Stack.Screen options={{ headerShown: false }} />
      <ScrollView
        contentInsetAdjustmentBehavior="automatic"
        automaticallyAdjustKeyboardInsets
        keyboardDismissMode="interactive"
        keyboardShouldPersistTaps="handled"
        contentContainerStyle={styles.content}>
        <View style={styles.column}>
          <View style={styles.hero}>
            <View
              style={[
                styles.tile,
                {
                  backgroundColor: palette.tint,
                  borderRadius: heroSize / 3.4,
                  height: heroSize,
                  width: heroSize,
                },
              ]}>
              <Symbol
                name="at"
                size={heroSize * 0.55 * 1.05}
                scalesWithText={false}
                color="onTint"
              />
            </View>
            <View style={styles.heroText}>
              <Text
                variant="largeTitle"
                weight="bold"
                align="center"
                accessibilityRole="header"
                style={styles.heroTitle}>
                {handleCopy.heroTitle}
              </Text>
              <WrappedText
                variant="callout"
                color="themeSecondaryLabel"
                align="center">
                {handleCopy.heroSubtitle}
              </WrappedText>
            </View>
          </View>
          <View>
            <Text
              variant="subheadline"
              weight="semibold"
              color="themeSecondaryLabel"
              style={styles.fieldLabel}>
              {handleCopy.yourHandle}
            </Text>
            <TextInput
              value={state.input}
              onChangeText={(text) => {
                model.changeInput(text);
              }}
              placeholder={handleCopy.placeholder}
              accessibilityLabel={handleCopy.yourHandle}
              placeholderTextColor={palette.placeholderText}
              selectionColor={palette.tint}
              autoCapitalize="none"
              autoCorrect={false}
              spellCheck={false}
              editable={editable}
              style={[
                fontOf("body"),
                styles.field,
                {
                  backgroundColor: palette.secondarySystemGroupedBackground,
                  color: palette.label,
                },
                editable ? undefined : styles.disabled,
              ]}
            />
            <View style={styles.hint}>
              {state.problem === undefined ? (
                <WrappedText variant="footnote" color="themeSecondaryLabel">
                  {handleHint(state.minimum)}
                </WrappedText>
              ) : (
                <WrappedText variant="footnote" color="systemRed">
                  {handleProblemCopy(state.problem, state.input, state.minimum)}
                </WrappedText>
              )}
            </View>
            {state.loading ||
            state.status.kind !== "editing" ||
            state.notice !== undefined ? (
              <Feedback model={model} state={state} />
            ) : null}
            <Text
              variant="footnote"
              color="themeSecondaryLabel"
              style={styles.footnote}>
              {handleCopy.chooseFootnote}
            </Text>
          </View>
        </View>
      </ScrollView>
      <ActionFooter>
        <GlassButton
          variant="glassProminent"
          size="large"
          font="body"
          fullWidth
          title={primaryButtonTitle("choose", state)}
          disabled={!canClaim(state) && !canCheck(state)}
          onPress={() => {
            if (canClaim(state)) model.claim();
            else model.check();
          }}
        />
        <Pressable
          accessibilityRole="button"
          accessibilityState={{ disabled: state.claiming !== undefined }}
          disabled={state.claiming !== undefined}
          onPress={() => {
            model.skip();
          }}
          style={({ pressed }) => [
            styles.skip,
            pressed || state.claiming !== undefined ? styles.dim : undefined,
          ]}>
          <Text variant="headline" color="tint">
            {handleCopy.skip}
          </Text>
        </Pressable>
      </ActionFooter>
    </View>
  );
}

/** Availability, suggestions and notices under the field. */
function Feedback({
  model,
  state,
}: {
  readonly model: HandleModel;
  readonly state: HandleState;
}) {
  const palette = useColors();
  const status = state.status;
  const pick = statusPick(status);
  return (
    <>
      {status.kind === "checking" || state.loading ? (
        <View style={[styles.feedback, styles.progress]}>
          <ActivityIndicator />
        </View>
      ) : null}
      {status.kind === "suggestions" ? (
        <>
          <WrappedText
            variant="subheadline"
            style={[styles.feedback, styles.inset]}>
            {handleCopy.taken(status.input)}
          </WrappedText>
          <View
            style={[
              styles.feedback,
              styles.suggestions,
              { backgroundColor: palette.secondarySystemGroupedBackground },
            ]}>
            {visibleSuggestions(status).map((handle) => (
              <View key={handle}>
                <Pressable
                  accessibilityRole="button"
                  accessibilityState={{ selected: pick === handle }}
                  onPress={() => {
                    model.select(handle);
                  }}
                  style={({ pressed }) => [
                    styles.suggestion,
                    pressed ? styles.dim : undefined,
                  ]}>
                  <HandleText handle={handle} style={styles.flex} />
                  {pick === handle ? <Symbol name="checkmark" /> : null}
                </Pressable>
                <View
                  style={[styles.divider, { backgroundColor: palette.divider }]}
                />
              </View>
            ))}
            {hasMoreSuggestions(status) ? (
              <LinkButton
                title={handleCopy.showMore}
                onPress={() => {
                  model.showMore();
                }}
                style={styles.showMore}
              />
            ) : null}
          </View>
        </>
      ) : null}
      {status.kind === "available" ? (
        <Text
          variant="subheadline"
          color="systemGreen"
          style={[styles.feedback, styles.inset]}>
          {handleCopy.available(status.handle)}
        </Text>
      ) : null}
      {status.kind === "noneFree" ? (
        <>
          <WrappedText
            variant="subheadline"
            style={[styles.feedback, styles.inset]}>
            {handleCopy.noneFree(status.input)}
          </WrappedText>
          <LinkButton
            title={handleCopy.lookAgain}
            onPress={() => {
              model.check();
            }}
            style={[styles.feedback, styles.inset]}
          />
        </>
      ) : null}
      {state.notice === undefined ? null : (
        <>
          <WrappedText
            variant="subheadline"
            color="systemRed"
            style={[styles.feedback, styles.inset]}>
            {handleNoticeCopy(state.notice)}
          </WrappedText>
          {canRetryNotice(state.notice) ? (
            <LinkButton
              title={handleCopy.retry}
              onPress={() => {
                model.retry();
              }}
              style={[styles.feedback, styles.inset]}
            />
          ) : null}
        </>
      )}
    </>
  );
}

/** A default-style SwiftUI `Button` outside a list: tinted body text. */
function LinkButton({
  title,
  onPress,
  style,
}: {
  readonly title: string;
  readonly onPress: () => void;
  readonly style?: object;
}) {
  return (
    <Pressable
      accessibilityRole="button"
      onPress={onPress}
      style={({ pressed }) => [
        styles.link,
        style,
        pressed ? styles.dim : undefined,
      ]}>
      <Text color="tint">{title}</Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  content: {
    alignItems: "center",
    paddingBottom: 24,
    paddingHorizontal: 16,
    paddingTop: 40,
  },
  column: { gap: 30, maxWidth: sizes.readableWidth, width: "100%" },
  hero: { alignItems: "center", gap: 20 },
  tile: {
    alignItems: "center",
    borderCurve: "continuous",
    justifyContent: "center",
  },
  heroText: { gap: 12 },
  heroTitle: { fontSize: 32 },
  fieldLabel: { fontSize: 14, marginBottom: 5, paddingHorizontal: 16 },
  field: {
    borderCurve: "continuous",
    borderRadius: radii.capsule,
    margin: 0,
    minHeight: 52,
    paddingHorizontal: 16,
    paddingVertical: 0,
  },
  disabled: { opacity: 0.5 },
  hint: { paddingHorizontal: 16, paddingTop: 10 },
  feedback: { marginTop: 10 },
  inset: { paddingHorizontal: 16 },
  progress: { alignItems: "flex-start", paddingHorizontal: 16 },
  suggestions: {
    borderCurve: "continuous",
    borderRadius: radii.tile,
    overflow: "hidden",
  },
  suggestion: {
    alignItems: "center",
    flexDirection: "row",
    padding: 16,
  },
  showMore: { padding: 16 },
  divider: { height: StyleSheet.hairlineWidth, marginLeft: 16 },
  flex: { flex: 1 },
  footnote: { paddingHorizontal: 16, paddingTop: 24 },
  link: { alignSelf: "flex-start" },
  skip: {
    alignItems: "center",
    justifyContent: "center",
    minHeight: 44,
  },
  dim: { opacity: 0.5 },
});
