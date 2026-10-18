import { Stack } from "expo-router";
import { ActivityIndicator, StyleSheet, View } from "react-native";

import { policyRows } from "@/core/permissions.ts";
import { DetailsErrorSection } from "@/features/details/DetailsRows";
import { useOwnDetailsStore } from "@/features/details/detailsStores";
import { permissionsCopy } from "@/features/permissions/copy";
import { type ActiveSession, useActiveSession } from "@/session";
import { FormRow, FormScreen, FormSection } from "@/ui/Form";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { form } from "@/ui/tokens";
import { useStore } from "@/ui/useStore";

export interface PermissionsScreenProps {
  readonly conversationId: string;
}

/**
 * Group permissions (`PermissionsFeatureView`): the All members / Admins only
 * presets (super admins only), a disabled Custom row, and the six current
 * policies. A preset makes nine ordered writes; after a failure the screen
 * shows the policies the group has now.
 */
export function PermissionsScreen({ conversationId }: PermissionsScreenProps) {
  const session = useActiveSession();
  if (session === undefined) {
    return (
      <FormScreen>
        <Stack.Screen options={{ title: permissionsCopy.title }} />
      </FormScreen>
    );
  }
  return (
    <PermissionsBody
      key={session.generation}
      session={session}
      conversationId={conversationId}
    />
  );
}

function PermissionsBody({
  session,
  conversationId,
}: {
  readonly session: ActiveSession;
  readonly conversationId: string;
}) {
  const store = useOwnDetailsStore(session, conversationId);
  const state = useStore(store);
  const preset = state.details?.preset;
  const disabled = state.busy || !store.canChangePermissionPreset;
  const presetRow = (title: string, detail: string, adminOnly: boolean) => (
    <PresetRow
      key={title}
      title={title}
      detail={detail}
      selected={preset === title}
      disabled={disabled}
      onPress={() => {
        store.send({ kind: "preset", adminOnly });
      }}
    />
  );
  return (
    <FormScreen>
      <Stack.Screen options={{ title: permissionsCopy.title }} />
      <FormSection
        header={permissionsCopy.header}
        footer={permissionsCopy.presetFooter}>
        {presetRow(
          permissionsCopy.allMembers,
          permissionsCopy.allMembersDetail,
          false,
        )}
        {presetRow(
          permissionsCopy.adminsOnly,
          permissionsCopy.adminsOnlyDetail,
          true,
        )}
        <PresetRow
          title={permissionsCopy.custom}
          detail={permissionsCopy.customDetail}
          selected={preset === permissionsCopy.custom}
          disabled
          dimmed={preset !== permissionsCopy.custom}
        />
      </FormSection>
      <FormSection
        header={permissionsCopy.currentPolicies}
        footer={permissionsCopy.policiesFooter}>
        {policyRows(state.details?.policies ?? []).map((row) => (
          <FormRow
            key={row.title}
            accessibilityLabel={row.title}
            accessibilityValue={{ text: row.value }}>
            <Text>{row.title}</Text>
            <Text
              color="themeSecondaryLabel"
              align="right"
              style={styles.value}>
              {row.value}
            </Text>
          </FormRow>
        ))}
      </FormSection>
      {state.busy ? (
        <FormSection>
          <FormRow style={styles.center}>
            <ActivityIndicator />
          </FormRow>
        </FormSection>
      ) : null}
      <DetailsErrorSection error={state.error} />
    </FormScreen>
  );
}

interface PresetRowProps {
  readonly title: string;
  readonly detail: string;
  readonly selected: boolean;
  readonly disabled: boolean;
  /** Custom is never a button; it shows at 60 percent unless selected. */
  readonly dimmed?: boolean;
  readonly onPress?: () => void;
}

/**
 * A preset choice: a radio symbol, the title and a footnote. Disabled presets
 * draw at 50 percent; Custom is not a button and shows at 60 percent unless
 * selected.
 */
function PresetRow({
  title,
  detail,
  selected,
  disabled,
  dimmed = false,
  onPress,
}: PresetRowProps) {
  return (
    <FormRow
      onPress={disabled ? undefined : onPress}
      accessibilityRole={onPress === undefined ? "text" : "button"}
      accessibilityLabel={`${title}, ${detail}`}
      accessibilityState={{ selected, disabled }}
      separatorInset={presetSeparatorInset}
      style={[
        styles.presetRow,
        dimmed ? styles.dimmed : undefined,
        disabled && onPress !== undefined ? styles.disabled : undefined,
      ]}>
      <View style={styles.iconColumn}>
        <Symbol
          name={selected ? "largecircle.fill.circle" : "circle"}
          textStyle="title3"
          color={selected ? "tint" : "themeSecondaryLabel"}
        />
      </View>
      <View style={styles.text}>
        <Text>{title}</Text>
        <Text
          variant="footnote"
          color="themeSecondaryLabel"
          style={styles.detail}>
          {detail}
        </Text>
      </View>
    </FormRow>
  );
}
PresetRow.separatorInset = (): number => presetSeparatorInset;

const presetSeparatorInset = form.rowInset + form.iconColumn + form.iconGap;

const styles = StyleSheet.create({
  iconColumn: { alignItems: "center", minWidth: form.iconColumn },
  // VStack spacing 4, less the extra leading of the 18 pt footnote line.
  text: { flex: 1, gap: 3 },
  value: { flex: 1 },
  presetRow: { paddingVertical: 14 },
  // Measured on iOS 27: wrapped footnote lines advance 17.9 pt here.
  detail: { lineHeight: 18 },
  dimmed: { opacity: 0.6 },
  // Native disabled plain buttons draw at about 50 percent (FormRow uses 40).
  disabled: { opacity: 0.5 },
  center: { justifyContent: "center" },
});
