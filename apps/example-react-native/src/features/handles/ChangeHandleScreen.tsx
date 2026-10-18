import { ActivityIndicator, Pressable, StyleSheet, View } from "react-native";

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
import { handleCopy } from "@/features/handles/copy";
import { useHandleModel } from "@/features/handles/useHandleModel";
import { type ActiveSession, useActiveSession } from "@/session";
import { ActionFooter } from "@/ui/ActionFooter";
import { useColors } from "@/ui/colors";
import { FormRow, FormScreen, FormSection } from "@/ui/Form";
import { TextFieldRow } from "@/ui/FormRows";
import { GlassButton } from "@/ui/GlassButton";
import { HandleText } from "@/ui/Identity";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";

/**
 * Change handle, pushed from Settings (`HandleView.swift`, mode `.change`):
 * a grouped form with the current handle, the new handle editor, the other
 * handles with "Use", and a footer that asks to confirm every claim.
 */
export function ChangeHandleScreen() {
  const session = useActiveSession();
  if (session === undefined) return null;
  return <ChangeHandle key={session.generation} session={session} />;
}

function ChangeHandle({ session }: { readonly session: ActiveSession }) {
  const palette = useColors();
  const { model, state } = useHandleModel(session, "change");
  const editable = isEditable(state);
  const status = state.status;
  const pick = statusPick(status);
  return (
    <View
      style={[
        styles.screen,
        { backgroundColor: palette.systemGroupedBackground },
      ]}>
      <FormScreen>
        {state.current === undefined ? null : (
          <FormSection header={handleCopy.currentHandle}>
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
        <FormSection header={handleCopy.newHandle}>
          <TextFieldRow
            value={state.input}
            onChangeText={(text) => {
              model.changeInput(text);
            }}
            placeholder={handleCopy.placeholder}
            accessibilityLabel={handleCopy.newHandle}
            autoCapitalize="none"
            autoCorrect={false}
            spellCheck={false}
            editable={editable}
          />
          <FormRow>
            {state.problem === undefined ? (
              <Text color="themeSecondaryLabel" style={styles.flex}>
                {handleHint(state.minimum)}
              </Text>
            ) : (
              <Text color="systemRed" style={styles.flex}>
                {handleProblemCopy(state.problem, state.input, state.minimum)}
              </Text>
            )}
          </FormRow>
          {status.kind === "checking" || state.loading ? (
            <FormRow>
              <ActivityIndicator />
            </FormRow>
          ) : null}
          {status.kind === "suggestions" ? (
            <FormRow>
              <Text variant="subheadline" style={styles.flex}>
                {handleCopy.taken(status.input)}
              </Text>
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
              <HandleText handle={handle} style={styles.flex} />
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
              <Text style={styles.flex}>
                {handleCopy.noneFree(status.input)}
              </Text>
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
              <Text color="systemRed" style={styles.flex}>
                {handleNoticeCopy(state.notice)}
              </Text>
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
              <Text style={styles.flex}>{handleCopy.otherHandlesDetail}</Text>
            </FormRow>
            {state.others.map((handle) => (
              <FormRow key={handle}>
                <HandleText handle={handle} style={styles.flex} />
                <Pressable
                  accessibilityRole="button"
                  accessibilityLabel={`${handleCopy.use}, ${handle}`}
                  disabled={!editable}
                  hitSlop={8}
                  onPress={() => {
                    model.switchBack(handle);
                  }}
                  style={({ pressed }) =>
                    pressed || !editable ? styles.dim : undefined
                  }>
                  <Text color="tint">{handleCopy.use}</Text>
                </Pressable>
              </FormRow>
            ))}
          </FormSection>
        )}
        <FormSection>
          <FormRow>
            <Text style={styles.flex}>{handleCopy.changeFootnote}</Text>
          </FormRow>
        </FormSection>
      </FormScreen>
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
  flex: { flex: 1 },
  dim: { opacity: 0.4 },
});
