import { Stack, router } from "expo-router";
import { useState } from "react";
import { StyleSheet } from "react-native";

import { detailsCopy } from "@/features/details/copy";
import { useSharedDetailsStore } from "@/features/details/detailsStores";
import { type ActiveSession, useActiveSession } from "@/session";
import { useColors } from "@/ui/colors";
import { FormScreen, FormSection } from "@/ui/Form";
import { TextFieldRow } from "@/ui/FormRows";

export interface EditGroupScreenProps {
  readonly conversationId: string;
}

/**
 * The Edit group sheet (`DetailsFeatureView` `.sheet(isPresented: $edit)`):
 * name and description fields, Cancel and Save. Save sends the edit to the
 * Details store and closes at once; errors show on Details.
 */
export function EditGroupScreen({ conversationId }: EditGroupScreenProps) {
  const session = useActiveSession();
  if (session === undefined) return <FormScreen />;
  return (
    <EditGroupForm
      key={session.generation}
      session={session}
      conversationId={conversationId}
    />
  );
}

function EditGroupForm({
  session,
  conversationId,
}: {
  readonly session: ActiveSession;
  readonly conversationId: string;
}) {
  const palette = useColors();
  const store = useSharedDetailsStore(session, conversationId);
  const [name, setName] = useState(
    () => store.getState().details?.record.name ?? "",
  );
  const [description, setDescription] = useState(
    () => store.getState().details?.description ?? "",
  );
  const close = () => {
    router.back();
  };
  return (
    <FormScreen>
      <Stack.Screen options={{ title: detailsCopy.editGroup }} />
      <Stack.Toolbar placement="left">
        <Stack.Toolbar.Button tintColor={palette.tint} onPress={close}>
          {detailsCopy.cancel}
        </Stack.Toolbar.Button>
      </Stack.Toolbar>
      <Stack.Toolbar placement="right">
        <Stack.Toolbar.Button
          variant="prominent"
          tintColor={palette.tint}
          onPress={() => {
            store.send({
              kind: "mutate",
              mutation: { kind: "edit", name, description },
            });
            close();
          }}>
          {detailsCopy.save}
        </Stack.Toolbar.Button>
      </Stack.Toolbar>
      <FormSection>
        <TextFieldRow
          placeholder={detailsCopy.groupName}
          value={name}
          onChangeText={setName}
        />
        <TextFieldRow
          placeholder={detailsCopy.description}
          value={description}
          onChangeText={setDescription}
          multiline
          scrollEnabled={false}
          style={styles.multiline}
        />
      </FormSection>
    </FormScreen>
  );
}

const styles = StyleSheet.create({
  // A vertical-axis TextField grows from one 52 pt line.
  multiline: { paddingBottom: 15, paddingTop: 15 },
});
