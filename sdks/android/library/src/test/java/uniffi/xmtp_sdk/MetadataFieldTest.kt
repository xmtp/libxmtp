package uniffi.xmtp_sdk

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// A smoke test of the metadata-field catalogue through the generated Kotlin
// records: descriptors, values, profiles and a typed denial cross the native
// boundary. Rust owns the field rules:
// xmtp_sdk/src/tests/metadata_fields/descriptors.rs::fields_are_identified_by_component_id,
// reads.rs::batch_reads_keep_request_order, profiles.rs::profile_writes_are_atomic_and_denials_are_typed,
// collections.rs::collection_fields_apply_whole_deltas.
class MetadataFieldTest {
    // verifies: META-069
    @Test
    fun wellKnownCatalogueFieldsReadAndWrite() =
        runBlocking {
            withTimeout(60_000) {
                withClients {
                    val alix = create()
                    val bo = create()
                    val groupName = metadataFieldRef(WellKnownMetadataField.GROUP_NAME)
                    val displayName = metadataFieldRef(WellKnownMetadataField.USER_DISPLAY_NAME)
                    assertEquals(MetadataFieldRef(0x800Cu, "USER_DISPLAY_NAME"), displayName)

                    // The default policy set lets all members write the
                    // name; delete stays super-admin only. Each member may
                    // write only its own profile entry.
                    val group = alix.conversations.createGroup(listOf(bo.inboxId()))
                    val descriptors = group.metadataFields().associateBy { it.field }
                    val allow = MetadataPolicy.Base(MetadataBasePolicy.Allow)
                    assertEquals(
                        MetadataFieldDescriptor(
                            groupName,
                            MetadataComponentType.String,
                            ComponentPermissions(
                                allow,
                                allow,
                                MetadataPolicy.Base(MetadataBasePolicy.AllowIfSuperAdmin),
                            ),
                            false,
                        ),
                        descriptors.getValue(groupName),
                    )
                    val selfOwned = MetadataPolicy.Base(MetadataBasePolicy.AllowIfSelfOrNonMember)
                    assertEquals(
                        MetadataFieldDescriptor(
                            displayName,
                            MetadataComponentType.Map(MetadataKeyType.INBOX_ID, MetadataScalarType.STRING),
                            ComponentPermissions(selfOwned, selfOwned, selfOwned),
                            true,
                        ),
                        descriptors.getValue(displayName),
                    )
                    assertEquals(groupName, group.metadataField("GROUP_NAME")?.field)

                    group.updateMetadataField(groupName, ComponentMutation.Replace(FieldValue.String("Team")))
                    bo.conversations.sync()
                    val boGroup = (checkNotNull(bo.conversations.getById(group.id())) as Conversation.Group).group
                    boGroup.sync()
                    assertEquals(MetadataValue.Scalar(FieldValue.String("Team")), boGroup.metadataValue(groupName))

                    boGroup.updateUserData(listOf(UserFieldUpdate(displayName, FieldValue.String("Bo"))))
                    group.sync()
                    val profiles = group.userData(null, null)
                    assertEquals(listOf(UserFieldValue(displayName, FieldValue.String("Bo"))), profiles[bo.inboxId()])
                    assertEquals(emptyList<UserFieldValue>(), profiles[alix.inboxId()])
                    assertEquals(FieldValue.String("Bo"), group.mapValue(displayName, FieldKey.InboxId(bo.inboxId())))
                    val profileMap =
                        MetadataValue.Map(listOf(MapEntry(FieldKey.InboxId(bo.inboxId()), FieldValue.String("Bo"))))
                    assertEquals(profileMap, group.metadataValue(displayName))
                    assertEquals(
                        listOf(
                            MetadataFieldValue(groupName, MetadataValue.Scalar(FieldValue.String("Team"))),
                            MetadataFieldValue(displayName, profileMap),
                        ),
                        group.metadataValues(listOf(groupName, displayName)),
                    )

                    val duplicate =
                        runCatching {
                            boGroup.updateUserData(
                                listOf(
                                    UserFieldUpdate(displayName, FieldValue.String("Bobby")),
                                    UserFieldUpdate(displayName, null),
                                ),
                            )
                        }.exceptionOrNull()
                    assertTrue("Expected DuplicateField, got $duplicate", duplicate is XmtpException.DuplicateField)
                    val details = (duplicate as XmtpException.DuplicateField).v1
                    assertEquals("DuplicateField", details.code)
                    assertEquals(ErrorCategory.INPUT, details.category)
                }
            }
        }

    // An application catalogue, and so the Set, byte-keyed Map and Unknown
    // component types and the And and Any policies, needs a backend catalogue
    // or the conformance build's override. These records still cross the
    // generated converters: ServerConfiguration.applicationComponents reads
    // the definition list, and metadataFields() reads the descriptor list.
    @Test
    fun catalogueRecordsKeepEachVariant() {
        val base = { policy: MetadataBasePolicy -> MetadataPolicy.Base(policy) }
        val nested =
            MetadataPolicy.Any(
                listOf(
                    MetadataPolicy.And(listOf(base(MetadataBasePolicy.AllowIfAdmin), base(MetadataBasePolicy.Deny))),
                    base(MetadataBasePolicy.Unknown(7)),
                ),
            )
        val types =
            listOf(
                MetadataComponentType.Bytes,
                MetadataComponentType.String,
                MetadataComponentType.Map(MetadataKeyType.BYTES, MetadataScalarType.BYTES),
                MetadataComponentType.Map(MetadataKeyType.INBOX_ID, MetadataScalarType.STRING),
                MetadataComponentType.Set(MetadataKeyType.BYTES),
                MetadataComponentType.Set(MetadataKeyType.INBOX_ID),
                MetadataComponentType.Unknown(99),
            )
        val permissions =
            listOf(
                ComponentPermissions(
                    base(MetadataBasePolicy.Allow),
                    base(MetadataBasePolicy.AllowIfSuperAdmin),
                    base(MetadataBasePolicy.AllowIfSelfOrNonMember),
                ),
                ComponentPermissions(nested, base(MetadataBasePolicy.Deny), MetadataPolicy.And(emptyList())),
            )
        val definitions =
            types.mapIndexed { index, type ->
                ApplicationComponentDefinition(
                    (0xC001 + index).toUShort(),
                    "field_$index",
                    type,
                    permissions[index % 2],
                    inGroups = index % 2 == 0,
                    inDms = index % 3 == 0,
                )
            }
        assertEquals(
            definitions,
            FfiConverterSequenceTypeApplicationComponentDefinition.lift(
                FfiConverterSequenceTypeApplicationComponentDefinition.lower(definitions),
            ),
        )
        val descriptors =
            definitions.mapIndexed { index, definition ->
                // A field this reader's catalogue does not name has no label.
                MetadataFieldDescriptor(
                    MetadataFieldRef(definition.componentId, if (index % 2 == 0) definition.name else null),
                    definition.componentType,
                    definition.permissions,
                    isUserField = index % 3 == 0,
                )
            }
        assertEquals(
            descriptors,
            FfiConverterSequenceTypeMetadataFieldDescriptor.lift(
                FfiConverterSequenceTypeMetadataFieldDescriptor.lower(descriptors),
            ),
        )

        // Byte arrays in generated enum variants compare by reference, so
        // compare them by content.
        val key = FieldKey.Bytes(byteArrayOf(0, -1))
        val set =
            FfiConverterTypeMetadataValue.lift(
                FfiConverterTypeMetadataValue.lower(MetadataValue.Set(listOf(key, FieldKey.InboxId("ab")))),
            ) as MetadataValue.Set
        assertArrayEquals(byteArrayOf(0, -1), (set.v1[0] as FieldKey.Bytes).v1)
        assertEquals(FieldKey.InboxId("ab"), set.v1[1])
        val map =
            FfiConverterTypeMetadataValue.lift(
                FfiConverterTypeMetadataValue.lower(
                    MetadataValue.Map(listOf(MapEntry(key, FieldValue.Bytes(byteArrayOf(4, 0))))),
                ),
            ) as MetadataValue.Map
        assertArrayEquals(byteArrayOf(0, -1), (map.v1.single().key as FieldKey.Bytes).v1)
        assertArrayEquals(byteArrayOf(4, 0), (map.v1.single().value as FieldValue.Bytes).v1)
    }

    // Collection mutations with byte keys need an app catalogue, which only the
    // conformance build can install. Their records still cross the converter.
    @Test
    fun byteCollectionMutationsKeepNestedBytes() {
        val key = FieldKey.Bytes(byteArrayOf(0, -1))
        val mutation =
            ComponentMutation.MapDelta(
                listOf(
                    MapMutation.Insert(key, FieldValue.Bytes(byteArrayOf(0, -128, -1))),
                    MapMutation.Update(FieldKey.Bytes(byteArrayOf(5)), FieldValue.Bytes(byteArrayOf(6, -6))),
                    MapMutation.Delete(key),
                ),
            )
        val restored =
            FfiConverterTypeComponentMutation.lift(FfiConverterTypeComponentMutation.lower(mutation))
                as ComponentMutation.MapDelta
        val insert = restored.v1[0] as MapMutation.Insert
        assertArrayEquals(byteArrayOf(0, -1), (insert.v1 as FieldKey.Bytes).v1)
        assertArrayEquals(byteArrayOf(0, -128, -1), (insert.v2 as FieldValue.Bytes).v1)
        val update = restored.v1[1] as MapMutation.Update
        assertArrayEquals(byteArrayOf(5), (update.v1 as FieldKey.Bytes).v1)
        assertArrayEquals(byteArrayOf(6, -6), (update.v2 as FieldValue.Bytes).v1)
        assertArrayEquals(byteArrayOf(0, -1), ((restored.v1[2] as MapMutation.Delete).v1 as FieldKey.Bytes).v1)
        val set =
            ComponentMutation.SetDelta(
                listOf(SetMutation.Insert(key), SetMutation.Delete(FieldKey.Bytes(byteArrayOf(9)))),
            )
        val restoredSet =
            FfiConverterTypeComponentMutation.lift(
                FfiConverterTypeComponentMutation.lower(set),
            ) as ComponentMutation.SetDelta
        assertArrayEquals(byteArrayOf(0, -1), ((restoredSet.v1[0] as SetMutation.Insert).v1 as FieldKey.Bytes).v1)
        assertArrayEquals(byteArrayOf(9), ((restoredSet.v1[1] as SetMutation.Delete).v1 as FieldKey.Bytes).v1)
    }
}
