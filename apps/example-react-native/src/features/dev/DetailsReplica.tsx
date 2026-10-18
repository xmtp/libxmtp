import { Stack } from "expo-router";
import { useState } from "react";
import { Pressable, StyleSheet, View } from "react-native";

import { commonCopy } from "@/copy/common";
import { galleryCopy } from "@/features/dev/copy";
import { Avatar } from "@/ui/Avatar";
import { FormRow, FormScreen, FormSection } from "@/ui/Form";
import {
  ButtonRow,
  NavigationRow,
  RadioRow,
  TextFieldRow,
  ToggleRow,
} from "@/ui/FormRows";
import { GlassSurface } from "@/ui/GlassSurface";
import { Symbol, type SymbolName } from "@/ui/Symbol";
import { Text } from "@/ui/Text";

const copy = galleryCopy.details;

/** A Details shortcut tile (`DetailsShortcutLabelStyle`). */
function ShortcutTile({
  title,
  systemImage,
}: {
  title: string;
  systemImage: SymbolName;
}) {
  return (
    <Pressable
      style={styles.tileWrap}
      accessibilityRole="button"
      accessibilityLabel={title}>
      <GlassSurface cornerRadius={20} interactive style={styles.tile}>
        <Symbol name={systemImage} textStyle="title3" color="accentText" />
        <Text variant="caption" weight="semibold" color="accentText">
          {title}
        </Text>
      </GlassSurface>
    </Pressable>
  );
}

/** A member row (`MemberIdentityRow`): 44 pt avatar, identity, role. */
function MemberRow({
  seed,
  title,
  role,
}: {
  seed: string;
  title: string;
  role: string;
}) {
  return (
    <FormRow onPress={() => {}} separatorInset={32.5} style={styles.memberRow}>
      <Avatar seed={seed} label={title.slice(0, 1).toUpperCase()} />
      <Text style={styles.flex}>{title}</Text>
      <Text variant="subheadline" color="secondaryLabel">
        {role}
      </Text>
    </FormRow>
  );
}

/**
 * Details with the content of `docs/screenshots/group-details.png`, plus the
 * other Details, Permissions and Disappearing row kinds.
 */
export function DetailsReplica() {
  const [alias, setAlias] = useState("");
  const [toggle, setToggle] = useState(true);
  const [preset, setPreset] = useState<"all" | "admins">("all");
  const [duration, setDuration] = useState<"hour" | "day">("hour");
  return (
    <FormScreen>
      <Stack.Screen
        options={{
          title: galleryCopy.sections.details,
          headerLargeTitleEnabled: false,
        }}
      />
      <Stack.Toolbar placement="right">
        <Stack.Toolbar.Button onPress={() => {}}>
          {commonCopy.edit}
        </Stack.Toolbar.Button>
      </Stack.Toolbar>
      <FormSection plain>
        <View style={styles.header}>
          <Avatar seed="lena" label="PD" size={96} />
          <Text variant="title" weight="bold" align="center">
            {copy.title}
          </Text>
          <Text variant="subheadline" color="secondaryLabel">
            {copy.subtitle}
          </Text>
          <View style={styles.tiles}>
            <ShortcutTile
              title={galleryCopy.addMember}
              systemImage="person.badge.plus"
            />
            <ShortcutTile
              title={galleryCopy.disappearingShort}
              systemImage="hourglass"
            />
          </View>
        </View>
      </FormSection>
      <FormSection header={copy.nameHeader} footer={copy.nameFooter}>
        <TextFieldRow
          placeholder={galleryCopy.localName}
          value={alias}
          onChangeText={setAlias}
        />
      </FormSection>
      <FormSection>
        <FormRow>
          <Text>{copy.notificationsOff}</Text>
        </FormRow>
      </FormSection>
      <FormSection header={copy.membersHeader}>
        <ButtonRow
          title={galleryCopy.addMember}
          systemImage="person.badge.plus"
          color="systemBlue"
          iconColor="tint"
          iconStyle="label"
          iconSize={24}
          onPress={() => {}}
        />
        <MemberRow seed="lena" title={copy.memberA} role={copy.member} />
        <MemberRow seed="priya" title={copy.memberB} role={copy.superAdmin} />
      </FormSection>
      <FormSection>
        <NavigationRow
          title={copy.permissions}
          systemImage="lock.shield"
          value={copy.permissionsValue}
          onPress={() => {}}
        />
        <NavigationRow
          title={copy.disappearing}
          systemImage="hourglass"
          value={copy.off}
          onPress={() => {}}
        />
        <NavigationRow
          title={copy.mediaAndFiles}
          systemImage="photo"
          onPress={() => {}}
        />
        <ToggleRow
          title={galleryCopy.notifications}
          value={toggle}
          onValueChange={setToggle}
        />
      </FormSection>
      <FormSection header={copy.permissions}>
        <RadioRow
          title={copy.allMembers}
          detail={copy.allMembersDetail}
          selected={preset === "all"}
          onPress={() => {
            setPreset("all");
          }}
        />
        <RadioRow
          title={copy.adminsOnly}
          detail={copy.adminsOnlyDetail}
          selected={preset === "admins"}
          onPress={() => {
            setPreset("admins");
          }}
        />
      </FormSection>
      <FormSection header={copy.disappearing}>
        <RadioRow
          indicator="checkmark"
          title={copy.oneHour}
          selectedLabel={copy.selected}
          selected={duration === "hour"}
          onPress={() => {
            setDuration("hour");
          }}
        />
        <RadioRow
          indicator="checkmark"
          title={copy.oneDay}
          selectedLabel={copy.selected}
          selected={duration === "day"}
          onPress={() => {
            setDuration("day");
          }}
        />
      </FormSection>
    </FormScreen>
  );
}

const styles = StyleSheet.create({
  header: { alignItems: "center", gap: 10 },
  tiles: { alignSelf: "stretch", flexDirection: "row", gap: 10, paddingTop: 6 },
  tileWrap: { flex: 1 },
  tile: {
    alignItems: "center",
    gap: 7,
    justifyContent: "center",
    minHeight: 68,
    paddingHorizontal: 8,
    paddingVertical: 12,
  },
  // SwiftUI adds `.padding(.vertical, 4)` inside the 15 pt row inset.
  memberRow: { paddingVertical: 19 },
  flex: { flex: 1 },
});
