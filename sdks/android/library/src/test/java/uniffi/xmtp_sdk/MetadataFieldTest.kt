package uniffi.xmtp_sdk

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

// Metadata-field catalogue records through the generated Kotlin converters.
// Rust owns the field rules and the live read and write paths:
// xmtp_sdk/src/tests/metadata_fields/descriptors.rs::fields_are_identified_by_component_id,
// reads.rs::batch_reads_keep_request_order, profiles.rs::profile_writes_are_atomic_and_denials_are_typed,
// collections.rs::collection_fields_apply_whole_deltas.
class MetadataFieldTest {
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
