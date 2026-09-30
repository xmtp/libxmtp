import uniffi.xmtp_sdk.*

// Alix and Bo read one group through different backend catalogues, so a
// name labels a field for one reader only and the component ID identifies
// it.
private val STATUS: UShort = 0xC001u
private val NICKNAME: UShort = 0xC002u
private val TOPIC: UShort = 0xC003u
private val AVATAR: UShort = 0xC004u
private val LATER: UShort = 0xC005u
private val DISPLAY_NAME: UShort = 0x800Cu

private val nicknameType = MetadataComponentType.Map(MetadataKeyType.INBOX_ID, MetadataScalarType.STRING)

private fun field(
    componentId: UShort,
    name: String? = null,
) = MetadataFieldRef(componentId, name)

private fun permissions(base: MetadataBasePolicy): ComponentPermissions {
    val policy = MetadataPolicy.Base(base)
    return ComponentPermissions(policy, policy, policy)
}

private fun definition(
    componentId: UShort,
    name: String,
    componentType: MetadataComponentType,
    base: MetadataBasePolicy,
    inDms: Boolean,
) = ApplicationComponentDefinition(componentId, name, componentType, permissions(base), inGroups = true, inDms = inDms)

// `later` has a type tag no SDK knows, so no conversation registers it.
private val alixCatalogue =
    listOf(
        definition(STATUS, "status", MetadataComponentType.String, MetadataBasePolicy.Allow, true),
        definition(NICKNAME, "nickname", nicknameType, MetadataBasePolicy.AllowIfSelfOrNonMember, true),
        definition(TOPIC, "topic", MetadataComponentType.String, MetadataBasePolicy.AllowIfAdmin, true),
        definition(AVATAR, "avatar", MetadataComponentType.Bytes, MetadataBasePolicy.Allow, false),
        definition(LATER, "later", MetadataComponentType.Unknown(99), MetadataBasePolicy.Allow, true),
    )

// Bo's catalogue gives `status` to another field and names STATUS after a
// well-known field, with a type and policy the group never committed.
private val boCatalogue =
    listOf(
        definition(STATUS, "GROUP_NAME", MetadataComponentType.Bytes, MetadataBasePolicy.Deny, true),
        definition(AVATAR, "status", MetadataComponentType.Bytes, MetadataBasePolicy.Allow, false),
    )

private fun application(vararg labels: String?) =
    listOf(
        MetadataFieldDescriptor(
            field(STATUS, labels[0]),
            MetadataComponentType.String,
            permissions(MetadataBasePolicy.Allow),
            false,
        ),
        MetadataFieldDescriptor(
            field(NICKNAME, labels[1]),
            nicknameType,
            permissions(MetadataBasePolicy.AllowIfSelfOrNonMember),
            true,
        ),
        MetadataFieldDescriptor(
            field(TOPIC, labels[2]),
            MetadataComponentType.String,
            permissions(MetadataBasePolicy.AllowIfAdmin),
            false,
        ),
        MetadataFieldDescriptor(
            field(AVATAR, labels[3]),
            MetadataComponentType.Bytes,
            permissions(MetadataBasePolicy.Allow),
            false,
        ),
    )

private suspend fun clientWith(
    catalogue: List<ApplicationComponentDefinition>,
    options: ClientOptions,
): SDKClient {
    sdkConformanceUseApplicationComponents(catalogue)
    try {
        return SDKClient.create(
            generateLocalSigner(),
            options.copy(storage = StorageOptions(location = StorageLocation.InMemory)),
        )
    } finally {
        sdkConformanceUseApplicationComponents(null)
    }
}

// Generated enum variants compare byte arrays by reference, so compare
// scalar values by content.
private fun FieldValue.show(): Any =
    when (this) {
        is FieldValue.Bytes -> v1.toList()
        is FieldValue.String -> v1
    }

private fun MetadataValue.show(): Any = if (this is MetadataValue.Scalar) v1.show() else this

private fun checkKind(
    thrown: Throwable?,
    code: String,
    category: ErrorCategory,
) {
    val details =
        when (thrown) {
            is XmtpException.PermissionDenied -> thrown.v1
            is XmtpException.DuplicateField -> thrown.v1
            is XmtpException.UnknownField -> thrown.v1
            else -> throw IllegalStateException("unexpected error $thrown")
        }
    check(details.code == code && details.category == category && !details.retryable) { "$details" }
}

internal suspend fun metadataFields(options: ClientOptions) {
    val alix = clientWith(alixCatalogue, options)
    val bo = clientWith(boCatalogue, options)
    check(alix.serverConfiguration().applicationComponents == alixCatalogue)
    val alixId = alix.inboxId()
    val boId = bo.inboxId()
    val group = alix.conversations().createGroup(listOf(boId))
    bo.conversations().sync()
    val boGroup = (bo.conversations().getById(group.id()) as Conversation.Group).group

    // Descriptors: the committed type and policies with each reader's labels.
    val displayName = metadataFieldRef(WellKnownMetadataField.USER_DISPLAY_NAME)
    val groupName = metadataFieldRef(WellKnownMetadataField.GROUP_NAME)
    check(displayName == field(DISPLAY_NAME, "USER_DISPLAY_NAME"))
    val alixFields = group.metadataFields()
    val wellKnown =
        listOf(
            groupName to false,
            field(0x8005u, "GROUP_DESCRIPTION") to false,
            field(0x8006u, "GROUP_IMAGE_URL") to false,
            field(0x8007u, "MESSAGE_DISAPPEAR_FROM_NS") to false,
            field(0x8008u, "MESSAGE_DISAPPEAR_IN_NS") to false,
            field(0x8009u, "APP_DATA") to false,
            displayName to true,
            field(0x800Du, "GROUP_IMAGE") to false,
        )
    check(alixFields.take(8).map { it.field to it.isUserField } == wellKnown) { "${alixFields.take(8)}" }
    check(alixFields.drop(8) == application("status", "nickname", "topic", "avatar")) { "${alixFields.drop(8)}" }
    val boFields = boGroup.metadataFields().drop(8)
    check(boFields == application("GROUP_NAME", null, null, "status")) { "$boFields" }
    check(group.metadataField("status")?.field == field(STATUS, "status"))
    check(boGroup.metadataField("status")?.field == field(AVATAR, "status"))
    check(boGroup.metadataField("GROUP_NAME")?.field == groupName)
    check(group.metadataField("later") == null)

    // Values: request order from one snapshot, each with the reader's label.
    boGroup.updateMetadataField(field(STATUS), ComponentMutation.Replace(FieldValue.String("hello")))
    group.sync()
    group.updateMetadataField(
        field(AVATAR, "avatar"),
        ComponentMutation.Replace(FieldValue.Bytes(byteArrayOf(1, 2, 3))),
    )
    group.updateMetadataField(groupName, ComponentMutation.Replace(FieldValue.String("Team")))
    boGroup.sync()
    val values =
        boGroup
            .metadataValues(listOf(field(AVATAR, "status"), field(STATUS, "status"), groupName, field(TOPIC)))
            .map { it.field to it.value?.show() }
    check(
        values ==
            listOf(
                field(AVATAR, "status") to listOf<Byte>(1, 2, 3),
                field(STATUS, "GROUP_NAME") to "hello",
                groupName to "Team",
                field(TOPIC) to null,
            ),
    ) { "$values" }
    check(boGroup.metadataValues(emptyList()).isEmpty())

    // Profiles: one commit for two fields; a repeat commits nothing.
    val before = boGroup.debugInfo().epoch
    boGroup.updateUserData(
        listOf(
            UserFieldUpdate(displayName, FieldValue.String("Bo")),
            UserFieldUpdate(field(NICKNAME), FieldValue.String("B")),
        ),
    )
    check(boGroup.debugInfo().epoch == before + 1u)
    boGroup.updateUserData(listOf(UserFieldUpdate(displayName, FieldValue.String("Bo"))))
    check(boGroup.debugInfo().epoch == before + 1u)
    group.sync()
    val boProfile =
        listOf(
            UserFieldValue(displayName, FieldValue.String("Bo")),
            UserFieldValue(field(NICKNAME, "nickname"), FieldValue.String("B")),
        )
    val everyone = group.userData(null, null)
    check(everyone == mapOf(alixId to emptyList(), boId to boProfile)) { "$everyone" }
    val noFields = group.userData(emptyList(), null)
    check(noFields == mapOf(alixId to emptyList<UserFieldValue>(), boId to emptyList())) { "$noFields" }
    check(group.userData(null, emptyList()).isEmpty())
    check(group.userData(listOf(field(NICKNAME)), listOf(boId)) == mapOf(boId to boProfile.drop(1)))

    // Denials are typed and commit nothing.
    checkKind(
        runCatching {
            boGroup.updateMetadataField(field(TOPIC), ComponentMutation.Replace(FieldValue.String("x")))
        }.exceptionOrNull(),
        "PermissionDenied",
        ErrorCategory.CONVERSATION,
    )
    checkKind(
        runCatching {
            boGroup.updateUserData(
                listOf(
                    UserFieldUpdate(displayName, FieldValue.String("Bobby")),
                    UserFieldUpdate(displayName, null),
                ),
            )
        }.exceptionOrNull(),
        "DuplicateField",
        ErrorCategory.INPUT,
    )
    check(boGroup.debugInfo().epoch == before + 1u)
    check(boGroup.metadataValue(field(TOPIC)) == null)

    // A DM holds the pair's profiles and its DM fields, never group-only ones.
    val dm = alix.conversations().createDm(boId)
    val dmIds = dm.metadataFields().map { it.field.componentId }
    check(DISPLAY_NAME in dmIds && NICKNAME in dmIds && AVATAR !in dmIds) { "$dmIds" }
    dm.updateUserData(listOf(UserFieldUpdate(displayName, FieldValue.String("Alix"))))
    val dmProfiles = dm.userData(null, null)
    check(
        dmProfiles ==
            mapOf(alixId to listOf(UserFieldValue(displayName, FieldValue.String("Alix"))), boId to emptyList()),
    ) { "$dmProfiles" }
    checkKind(
        runCatching {
            dm.updateMetadataField(field(AVATAR), ComponentMutation.Replace(FieldValue.Bytes(byteArrayOf(1))))
        }.exceptionOrNull(),
        "UnknownField",
        ErrorCategory.INPUT,
    )

    alix.end()
    bo.end()
}
