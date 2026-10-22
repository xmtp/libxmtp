import { useState } from "react";
import { ActivityIndicator, StyleSheet, View } from "react-native";

import { abbreviatedDate } from "@/core/dates.ts";
import { canRetryNotice } from "@/core/handleModel.ts";
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
import { splitHandle } from "@/core/titles.ts";
import { handleCopy } from "@/features/handles/copy";
import { useHandleModel } from "@/features/handles/useHandleModel";
import { type ActiveSession, useActiveSession } from "@/session";
import { ActionFooter } from "@/ui/ActionFooter";
import { AlertDialog } from "@/ui/AlertDialog";
import { useColors } from "@/ui/colors";
import { FormRow, FormScreen, FormSection } from "@/ui/Form";
import { TextFieldRow } from "@/ui/FormRows";
import { GlassButton } from "@/ui/GlassButton";
import { HandleText } from "@/ui/Identity";
import { Symbol } from "@/ui/Symbol";
import { Span, Text } from "@/ui/Text";
import { WrappedText } from "@/ui/WrappedText";

/**
 * Change handle, pushed from Settings (mode `change`): a grouped form with the
 * current handle, the new handle editor, the other handles with "Use", and a
 * footer that asks to confirm every claim.
 */
export function ChangeHandleScreen() {
  const session = useActiveSession();
  if (session === undefined) return null;
  return <ChangeHandle key={session.generation} session={session} />;
}

function ChangeHandle({ session }: { readonly session: ActiveSession }) {
  const palette = useColors();
  const { model, state, confirmation } = useHandleModel(session, "change");
  const editable = isEditable(state);
  // The form scrolls under the footer, like `.safeAreaInset(edge: .bottom)`.
  const [footerHeight, setFooterHeight] = useState(0);
  const status = state.status;
  const pick = statusPick(status);
  return (
    <View
      style={[
        styles.screen,
        { backgroundColor: palette.systemGroupedBackground },
      ]}>
      <FormScreen bottomInset={footerHeight}>
        {state.current === undefined ? null : (
          <FormSection
            header={handleCopy.currentHandle}
            style={styles.firstHeader}>
            <FormRow>
              <HandleText handle={state.current} />
            </FormRow>
            {state.currentSince === undefined ? null : (
              <FormRow>
                <Text color="themeSecondaryLabel">
                  {handleCopy.yoursSince(abbreviatedDate(state.currentSince))}
                </Text>
              </FormRow>
            )}
          </FormSection>
        )}
        <FormSection
          header={handleCopy.newHandle}
          style={state.current === undefined ? styles.firstHeader : undefined}>
          <TextFieldRow
            value={state.input}
            onChangeText={(text) => {
              model.changeInput(text);
            }}
            placeholder={handleCopy.placeholder}
            accessibilityLabel={handleCopy.placeholder}
            autoCapitalize="none"
            autoCorrect={false}
            spellCheck={false}
            editable={editable}
          />
          <FormRow>
            {state.problem === undefined ? (
              <WrappedText color="themeSecondaryLabel" style={styles.flex}>
                {handleHint(state.minimum)}
              </WrappedText>
            ) : (
              <WrappedText color="systemRed" style={styles.flex}>
                {handleProblemCopy(state.problem, state.input, state.minimum)}
              </WrappedText>
            )}
          </FormRow>
          {status.kind === "checking" || state.loading ? (
            <FormRow>
              <ActivityIndicator />
            </FormRow>
          ) : null}
          {status.kind === "suggestions" ? (
            <FormRow>
              <WrappedText variant="subheadline" style={styles.flex}>
                {handleCopy.taken(status.input)}
              </WrappedText>
            </FormRow>
          ) : null}
          {status.kind === "available" ? (
            <FormRow>
              <Text color="systemGreen" style={styles.flex}>
                {handleCopy.available(status.handle)}
              </Text>
            </FormRow>
          ) : null}
          {visibleSuggestions(status).map((handle) => (
            <FormRow
              key={handle}
              accessibilityState={{ selected: pick === handle }}
              onPress={() => {
                model.select(handle);
              }}>
              <TintedHandle handle={handle} />
              {pick === handle ? (
                <Symbol name="checkmark" color="tint" />
              ) : null}
            </FormRow>
          ))}
          {hasMoreSuggestions(status) ? (
            <LinkRow
              title={handleCopy.showMore}
              onPress={() => {
                model.showMore();
              }}
            />
          ) : null}
          {status.kind === "noneFree" ? (
            <FormRow>
              <WrappedText style={styles.flex}>
                {handleCopy.noneFree(status.input)}
              </WrappedText>
            </FormRow>
          ) : null}
          {status.kind === "noneFree" ? (
            <LinkRow
              title={handleCopy.lookAgain}
              onPress={() => {
                model.check();
              }}
            />
          ) : null}
          {state.notice === undefined ? null : (
            <FormRow>
              <WrappedText color="systemRed" style={styles.flex}>
                {handleNoticeCopy(state.notice)}
              </WrappedText>
            </FormRow>
          )}
          {state.notice?.kind !== "skipFailed" &&
          canRetryNotice(state.notice) ? (
            <LinkRow
              title={handleCopy.retry}
              onPress={() => {
                model.retry();
              }}
            />
          ) : null}
        </FormSection>
        {state.others.length === 0 ? null : (
          <FormSection header={handleCopy.otherHandles}>
            <FormRow>
              <WrappedText style={styles.flex}>
                {handleCopy.otherHandlesDetail}
              </WrappedText>
            </FormRow>
            {state.others.map((handle) => (
              // A Form row with one button: the whole row is the button. When
              // the editor is blocked only the "Use" label dims, like SwiftUI.
              <FormRow
                key={handle}
                accessibilityRole="button"
                accessibilityLabel={`${handleCopy.use}, ${handle}`}
                accessibilityState={{ disabled: !editable }}
                {...(editable
                  ? {
                      onPress: () => {
                        model.switchBack(handle);
                      },
                    }
                  : {})}>
                <HandleText handle={handle} style={styles.flex} />
                <Text color="tint" style={editable ? undefined : styles.dim}>
                  {handleCopy.use}
                </Text>
              </FormRow>
            ))}
          </FormSection>
        )}
        <FormSection>
          <FormRow>
            <WrappedText style={styles.flex}>
              {handleCopy.changeFootnote}
            </WrappedText>
          </FormRow>
        </FormSection>
      </FormScreen>
      <View
        style={styles.footer}
        onLayout={(event) => {
          setFooterHeight(event.nativeEvent.layout.height);
        }}>
        <ActionFooter>
          <GlassButton
            variant="glassProminent"
            size="large"
            font="body"
            fullWidth
            title={primaryButtonTitle("change", state)}
            disabled={!canClaim(state) && !canCheck(state)}
            onPress={() => {
              if (canClaim(state)) model.claim();
              else model.check();
            }}
          />
        </ActionFooter>
      </View>
      <AlertDialog {...confirmation} />
    </View>
  );
}

/**
 * A handle inside a Form button label: the tint replaces the primary color,
 * so the name is tinted and the `#code` is the tint at the secondary level.
 */
function TintedHandle({ handle }: { readonly handle: string }) {
  const { name, code } = splitHandle(handle);
  return (
    <Text style={styles.flex} accessibilityLabel={handle}>
      <Span color="tint">{name}</Span>
      {code === "" ? null : <Span color="tintSecondary">{code}</Span>}
    </Text>
  );
}

/** A default-style `Button` row in a form: tinted text. */
function LinkRow({
  title,
  onPress,
}: {
  readonly title: string;
  readonly onPress: () => void;
}) {
  return (
    <FormRow onPress={onPress} accessibilityLabel={title}>
      <Text color="tint" style={styles.flex}>
        {title}
      </Text>
    </FormRow>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  footer: { bottom: 0, left: 0, position: "absolute", right: 0 },
  // Measured on iOS 27: the first header sits 10 pt under the large
  // title here (the form kit's first-header default is 8.7).
  firstHeader: { paddingTop: 10 },
  flex: { flex: 1 },
  dim: { opacity: 0.4 },
});
