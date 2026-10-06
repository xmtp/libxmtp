package uniffi.xmtp_sdk

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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

                    val group = alix.conversations().createGroup(listOf(bo.inboxId()))
                    val descriptors = group.metadataFields().associateBy { it.field }
                    val name = descriptors.getValue(groupName)
                    assertEquals(MetadataComponentType.String, name.componentType)
                    assertFalse("GROUP_NAME read as a user field", name.isUserField)
                    assertTrue(
                        "USER_DISPLAY_NAME did not read as a user field",
                        descriptors.getValue(displayName).isUserField,
                    )
                    assertEquals(groupName, group.metadataField("GROUP_NAME")?.field)

                    group.updateMetadataField(groupName, ComponentMutation.Replace(FieldValue.String("Team")))
                    bo.conversations().sync()
                    val boGroup = (checkNotNull(bo.conversations().getById(group.id())) as Conversation.Group).group
                    boGroup.sync()
                    assertEquals(MetadataValue.Scalar(FieldValue.String("Team")), boGroup.metadataValue(groupName))

                    boGroup.updateUserData(listOf(UserFieldUpdate(displayName, FieldValue.String("Bo"))))
                    group.sync()
                    val profiles = group.userData(null, null)
                    assertEquals(listOf(UserFieldValue(displayName, FieldValue.String("Bo"))), profiles[bo.inboxId()])
                    assertEquals(emptyList<UserFieldValue>(), profiles[alix.inboxId()])
                    assertEquals(FieldValue.String("Bo"), group.mapValue(displayName, FieldKey.InboxId(bo.inboxId())))
                    val values = group.metadataValues(listOf(groupName, displayName)).map { it.field }
                    assertEquals(listOf(groupName, displayName), values)

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

    // Collection mutations with byte keys need an app catalogue, which only the
    // conformance build can install. Their records still cross the converter.
    @Test
    fun byteCollectionMutationsKeepNestedBytes() {
        val key = FieldKey.Bytes(byteArrayOf(0, -1))
        val mutation =
            ComponentMutation.MapDelta(
                listOf(
                    MapMutation.Insert(key, FieldValue.Bytes(byteArrayOf(0, -128, -1))),
                    MapMutation.Delete(key),
                ),
            )
        val restored =
            FfiConverterTypeComponentMutation.lift(FfiConverterTypeComponentMutation.lower(mutation))
                as ComponentMutation.MapDelta
        val insert = restored.v1[0] as MapMutation.Insert
        assertArrayEquals(byteArrayOf(0, -1), (insert.v1 as FieldKey.Bytes).v1)
        assertArrayEquals(byteArrayOf(0, -128, -1), (insert.v2 as FieldValue.Bytes).v1)
        assertArrayEquals(byteArrayOf(0, -1), ((restored.v1[1] as MapMutation.Delete).v1 as FieldKey.Bytes).v1)
        val set = ComponentMutation.SetDelta(listOf(SetMutation.Insert(key)))
        val restoredSet =
            FfiConverterTypeComponentMutation.lift(
                FfiConverterTypeComponentMutation.lower(set),
            ) as ComponentMutation.SetDelta
        assertArrayEquals(byteArrayOf(0, -1), ((restoredSet.v1.single() as SetMutation.Insert).v1 as FieldKey.Bytes).v1)
    }
}
