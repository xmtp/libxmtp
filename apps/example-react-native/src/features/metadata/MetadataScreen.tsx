import { Stack } from "expo-router";
import { useEffect, useState } from "react";
import { ActivityIndicator, Platform, StyleSheet, View } from "react-native";

import {
  allows,
  canonicalKey,
  canSet,
  canWrite,
  draftChanged,
  draftChanges,
  draftValue,
  emptyDraft,
  entryAction,
  fieldHeader,
  isImmutable,
  markSaving,
  type MetadataEdit,
  type MetadataField,
  MetadataStore,
  mergeDraft,
  type OwnFieldDraft,
  type OwnFieldValue,
  readOnlyValue,
  setDraft,
} from "@/core/metadata.ts";
import { DetailsErrorSection } from "@/features/details/DetailsRows";
import { metadataCopy } from "@/features/metadata/copy";
import { SdkMetadataQueries } from "@/features/metadata/metadataQueries";
import { type ActiveSession, useActiveSession } from "@/session";
import { useColors } from "@/ui/colors";
import { FormRow, FormScreen, FormSection } from "@/ui/Form";
import { LabeledRow, TextFieldRow } from "@/ui/FormRows";
import { GlassButton, GlassIconButton } from "@/ui/GlassButton";
import { IdentityText } from "@/ui/Identity";
import { Text } from "@/ui/Text";
import { useToolbarIcon } from "@/ui/toolbarIcon";
import { toolbarTextItem } from "@/ui/ToolbarTextButton";
import { useStore } from "@/ui/useStore";

/**
 * The separator under a row of glass buttons starts at the first button's
 * title, 28 pt into the card (measured on iOS 27).
 */
const buttonRowInset = 28;

export interface MetadataScreenProps {
  readonly conversationId: string;
  /** My fields (`true`) or Group fields (`false`). */
  readonly own: boolean;
}

/**
 * Group fields and My fields: the raw registry fields of a conversation with
 * editors that follow each field's shape and policies.
 */
export function MetadataScreen({ conversationId, own }: MetadataScreenProps) {
  const session = useActiveSession();
  const title = own ? metadataCopy.myFields : metadataCopy.groupFields;
  if (session === undefined) {
    return (
      <FormScreen>
        <Stack.Screen options={{ title }} />
      </FormScreen>
    );
  }
  return (
    <MetadataBody
      key={session.generation}
      session={session}
      conversationId={conversationId}
      own={own}
    />
  );
}

function useMetadataStore(
  session: ActiveSession,
  conversationId: string,
): MetadataStore {
  const create = () => {
    const generation = session.generation;
    const valid = () => session.isCurrent(generation);
    return new MetadataStore(
      new SdkMetadataQueries(session, conversationId, valid),
      {
        admit: valid,
        run: (job) => {
          void session.scope.add(() => job());
        },
      },
    );
  };
  const [store, setStore] = useState(create);
  useEffect(() => {
    if (store.disposed) {
      setStore(create());
      return;
    }
    store.refresh();
    return () => {
      store.dispose();
    };
  }, [store, session, conversationId]);
  return store;
}

function MetadataBody({
  session,
  conversationId,
  own,
}: {
  readonly session: ActiveSession;
  readonly conversationId: string;
  readonly own: boolean;
}) {
  const palette = useColors();
  const reloadIcon = useToolbarIcon("arrow.clockwise");
  const store = useMetadataStore(session, conversationId);
  const state = useStore(store);
  const identities = useStore(session.identities);
  const [draft, setDraftState] = useState<OwnFieldDraft>(emptyDraft);
  const [inputs, setInputs] = useState<ReadonlyMap<number, GroupInputs>>(
    new Map(),
  );

  // Committed values merge into the draft; unsaved edits survive a reload.
  const snapshot = state.snapshot;
  useEffect(() => {
    setDraftState((current) => mergeDraft(current, snapshot.fields));
  }, [snapshot]);

  const memberIds = snapshot.members.map((member) => member.inboxId).join(",");
  useEffect(() => {
    if (memberIds !== "") void session.identities.load(memberIds.split(","));
  }, [session, memberIds]);

  const changes = draftChanges(draft);
  const fields = snapshot.fields.filter((field) => field.userField === own);
  const send = (edit: MetadataEdit) => {
    store.edit(edit);
  };

  return (
    <FormScreen contentContainerStyle={styles.content}>
      <Stack.Screen
        options={{
          title: own ? metadataCopy.myFields : metadataCopy.groupFields,
        }}
      />
      <Stack.Toolbar placement="right">
        {own
          ? toolbarTextItem(
              {
                title: metadataCopy.save,
                prominent: true,
                disabled: state.busy || changes.size === 0,
                onPress: () => {
                  if (changes.size === 0) return;
                  setDraftState(markSaving);
                  send({ kind: "own", changes });
                },
              },
              palette,
            )
          : null}
        {Platform.OS === "ios" ? (
          // SwiftUI draws this `.glass` button as a glass capsule inside the
          // bar item's own glass; a bar button would be a single circle.
          <Stack.Toolbar.View>
            <GlassIconButton
              systemImage="arrow.clockwise"
              shape="capsule"
              accessibilityLabel={metadataCopy.reload}
              disabled={state.busy}
              onPress={() => {
                store.refresh();
              }}
            />
          </Stack.Toolbar.View>
        ) : (
          <Stack.Toolbar.Button
            icon={reloadIcon}
            tintColor={palette.tint}
            accessibilityLabel={metadataCopy.reload}
            disabled={state.busy}
            onPress={() => {
              store.refresh();
            }}
          />
        )}
      </Stack.Toolbar>
      <FormSection>
        <FormRow>
          <Text color="secondaryLabel" style={styles.flex}>
            {own ? metadataCopy.ownIntro : metadataCopy.groupIntro}
          </Text>
        </FormRow>
      </FormSection>
      {fields.map((field) => (
        <FormSection key={field.id} header={fieldHeader(field)}>
          {own
            ? ownFieldRows(field, draft, (value) => {
                setDraftState((current) => setDraft(current, field.id, value));
              })
            : groupFieldRows(
                field,
                inputs.get(field.id) ?? emptyInputs,
                (change) => {
                  setInputs((current) =>
                    new Map(current).set(field.id, {
                      ...(current.get(field.id) ?? emptyInputs),
                      ...change,
                    }),
                  );
                },
                send,
              )}
          <FormRow>
            <Text variant="caption" color="secondaryLabel" style={styles.flex}>
              {field.policy}
            </Text>
          </FormRow>
        </FormSection>
      ))}
      {own && snapshot.members.length > 0 ? (
        <FormSection header={metadataCopy.otherMembers}>
          {snapshot.members.map((member) => (
            <FormRow key={member.inboxId} style={styles.member}>
              <View style={styles.memberText}>
                <IdentityText
                  inboxId={member.inboxId}
                  handle={identities.get(member.inboxId)?.handle}
                />
                {member.fields.map((field) => (
                  <Text
                    key={field.id}
                    variant="subheadline"
                    color="secondaryLabel">
                    {`${field.name}: ${field.present ? field.scalar : metadataCopy.absent}`}
                  </Text>
                ))}
              </View>
            </FormRow>
          ))}
        </FormSection>
      ) : null}
      {snapshot.missing.length > 0 ? (
        <FormSection header={metadataCopy.offered}>
          {snapshot.missing.map((name) => (
            <FormRow key={name}>
              <Text style={styles.flex}>{name}</Text>
            </FormRow>
          ))}
          <FormRow>
            <Text color="secondaryLabel" style={styles.flex}>
              {metadataCopy.offeredDetail}
            </Text>
          </FormRow>
        </FormSection>
      ) : null}
      <DetailsErrorSection error={state.error} />
      {state.busy ? (
        <FormSection>
          <FormRow style={styles.center}>
            <ActivityIndicator />
          </FormRow>
        </FormSection>
      ) : null}
    </FormScreen>
  );
}

/** A field that cannot be written: its value and the reason. */
function readOnlyRows(field: MetadataField) {
  return [
    <FormRow key="value">
      <Text style={styles.flex}>{readOnlyValue(field)}</Text>
    </FormRow>,
    <FormRow key="reason">
      <Text color="secondaryLabel" style={styles.flex}>
        {isImmutable(field) && field.componentPresent
          ? metadataCopy.immutable
          : metadataCopy.unsupported}
      </Text>
    </FormRow>,
  ];
}

/**
 * The rows of an own field. They are returned as an array so the section
 * draws a separator between each of them.
 */
function ownFieldRows(
  field: MetadataField,
  draft: OwnFieldDraft,
  onChange: (value: OwnFieldValue) => void,
) {
  if (!canWrite(field)) return readOnlyRows(field);
  const value = draftValue(draft, field.id);
  const status = draftChanged(draft, field.id)
    ? metadataCopy.changed
    : value.kind === "absent"
      ? metadataCopy.absent
      : metadataCopy.saved;
  return [
    <TextFieldRow
      key="field"
      placeholder={field.name}
      value={value.kind === "value" ? value.text : ""}
      onChangeText={(text) => {
        onChange({ kind: "value", text });
      }}
      editable={canSet(field)}
      autoCapitalize="none"
      autoCorrect={false}
      multiline
      scrollEnabled={false}
      style={styles.multiline}
    />,
    <FormRow key="status">
      <Text variant="caption" color="secondaryLabel" style={styles.flex}>
        {status}
      </Text>
    </FormRow>,
    <FormRow
      key="buttons"
      style={styles.buttons}
      separatorInset={buttonRowInset}>
      <GlassButton
        title={metadataCopy.setEmpty}
        disabled={!canSet(field)}
        onPress={() => {
          onChange({ kind: "value", text: "" });
        }}
      />
      <GlassButton
        title={metadataCopy.clear}
        disabled={!allows(field, "delete")}
        onPress={() => {
          onChange({ kind: "absent" });
        }}
      />
    </FormRow>,
  ];
}

function safely<T>(run: () => T, fallback: T): T {
  try {
    return run();
  } catch {
    return fallback;
  }
}

interface GroupInputs {
  readonly value: string;
  readonly key: string;
  readonly entryValue: string;
}

const emptyInputs: GroupInputs = { value: "", key: "", entryValue: "" };

/** The rows of a group field editor (see `ownFieldRows`). */
function groupFieldRows(
  field: MetadataField,
  { value, key, entryValue }: GroupInputs,
  setInputs: (change: Partial<GroupInputs>) => void,
  send: (edit: MetadataEdit) => void,
) {
  const setValue = (text: string) => {
    setInputs({ value: text });
  };
  const setKey = (text: string) => {
    setInputs({ key: text });
  };
  const setEntryValue = (text: string) => {
    setInputs({ entryValue: text });
  };
  if (!canWrite(field)) return readOnlyRows(field);
  if (field.shape === "string" || field.shape === "bytes") {
    return [
      <LabeledRow
        key="current"
        label={metadataCopy.currentValue}
        value={field.present ? field.scalar : metadataCopy.absent}
      />,
      <TextFieldRow
        key="field"
        placeholder={field.name}
        value={value}
        onChangeText={setValue}
        autoCapitalize="none"
        autoCorrect={false}
        multiline
        scrollEnabled={false}
        style={styles.multiline}
      />,
      <FormRow
        key="buttons"
        style={styles.buttons}
        separatorInset={buttonRowInset}>
        <GlassButton
          variant="glassProminent"
          title={metadataCopy.set}
          disabled={!canSet(field)}
          onPress={() => {
            send({ kind: "scalar", id: field.id, value });
          }}
        />
        <GlassButton
          title={metadataCopy.clear}
          disabled={!allows(field, "delete")}
          onPress={() => {
            send({ kind: "scalar", id: field.id, value: undefined });
          }}
        />
      </FormRow>,
    ];
  }
  const canonical = safely(() => canonicalKey(key, field.shape), key);
  const action = safely(() => entryAction(key, field), "insert" as const);
  const map = field.shape === "byteMap";
  return [
    ...field.entries.map((entry) => (
      <FormRow key={`entry.${entry.key}`}>
        <View style={styles.flex}>
          <Text variant="footnote" monospaced>
            {entry.key}
          </Text>
          {map ? (
            <Text variant="footnote" monospaced>
              {entry.value}
            </Text>
          ) : null}
        </View>
        {map ? (
          <GlassButton
            title={metadataCopy.edit}
            onPress={() => {
              setInputs({ key: entry.key, entryValue: entry.value });
            }}
          />
        ) : null}
        <GlassIconButton
          systemImage="trash"
          accessibilityLabel={metadataCopy.deleteEntry}
          disabled={!allows(field, "delete")}
          onPress={() => {
            send({
              kind: "entry",
              id: field.id,
              action: "delete",
              key: entry.key,
              value: "",
            });
          }}
        />
      </FormRow>
    )),
    <TextFieldRow
      key="key"
      placeholder={
        field.shape === "inboxSet"
          ? metadataCopy.inboxId
          : metadataCopy.keyInHex
      }
      value={key}
      onChangeText={setKey}
      autoCapitalize="none"
      autoCorrect={false}
    />,
    ...(map
      ? [
          <TextFieldRow
            key="value"
            placeholder={metadataCopy.valueInHex}
            value={entryValue}
            onChangeText={setEntryValue}
            autoCapitalize="none"
            autoCorrect={false}
          />,
        ]
      : []),
    <FormRow key="add" style={styles.buttons} separatorInset={buttonRowInset}>
      <GlassButton
        variant="glassProminent"
        title={action === "update" ? metadataCopy.update : metadataCopy.add}
        disabled={
          !allows(field, action) ||
          (!map && field.entries.some((entry) => entry.key === canonical))
        }
        onPress={() => {
          send({
            kind: "entry",
            id: field.id,
            action,
            key: canonical,
            value: entryValue,
          });
        }}
      />
    </FormRow>,
  ];
}

const styles = StyleSheet.create({
  // An inset-grouped list starts a headerless first section lower.
  content: { paddingTop: 35 },
  flex: { flex: 1 },
  center: { justifyContent: "center" },
  multiline: { paddingBottom: 15, paddingTop: 15 },
  // A glass button row in a list: 14.3 pt above and 13.3 pt below the glass (iOS 27).
  buttons: { gap: 8, paddingVertical: 15 },
  member: { paddingVertical: 19 },
  memberText: { flex: 1, gap: 5 },
});
