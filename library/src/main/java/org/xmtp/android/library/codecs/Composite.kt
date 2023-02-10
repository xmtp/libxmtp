package org.xmtp.android.library.codecs

import org.xmtp.proto.message.contents.CompositeKt.part
import org.xmtp.proto.message.contents.CompositeOuterClass
import org.xmtp.proto.message.contents.CompositeOuterClass.Composite.Part
import org.xmtp.proto.message.contents.composite
import org.xmtp.proto.message.contents.encodedContent

typealias Composite = org.xmtp.proto.message.contents.CompositeOuterClass.Composite

val ContentTypeComposite = ContentTypeIdBuilder.builderFromAuthorityId(
    authorityId = "xmtp.org",
    typeId = "composite",
    versionMajor = 1,
    versionMinor = 0
)

class CompositePartBuilder {
    companion object {
        fun buildFromEncodedContent(encodedContent: EncodedContent): CompositeOuterClass.Composite.Part {
            return CompositeOuterClass.Composite.Part.newBuilder().also {
                it.part = encodedContent
            }.build()
        }

        fun buildFromComosite(composite: Composite): CompositeOuterClass.Composite.Part {
            return CompositeOuterClass.Composite.Part.newBuilder().also {
                it.composite = composite
            }.build()
        }
    }
}

class CompositeCodec : ContentCodec<DecodedComposite> {
    override val contentType: ContentTypeId
        get() = ContentTypeComposite

    override fun encode(content: DecodedComposite): EncodedContent {
        val composite = toComposite(content)
        return EncodedContent.newBuilder().also {
            it.type = ContentTypeComposite
            it.content = composite.toByteString()
        }.build()
    }

    override fun decode(content: EncodedContent): DecodedComposite {
        val composite = Composite.parseFrom(content.content)
        return fromComposite(composite = composite)
    }

    private fun toComposite(decodedComposite: DecodedComposite): Composite {
        return Composite.newBuilder().also {
            val content = decodedComposite.encodedContent
            if (content != null) {
                it.addParts(CompositePartBuilder.buildFromEncodedContent(content))
                return it.build()
            }
            for (part in decodedComposite.parts) {
                val encodedContent = part.encodedContent
                if (encodedContent != null) {
                    it.addParts((CompositePartBuilder.buildFromEncodedContent(encodedContent)))
                } else {
                    it.addParts((CompositePartBuilder.buildFromComosite(toComposite(part))))
                }
            }
        }.build()
    }

    private fun fromComposite(composite: Composite): DecodedComposite {
        val decodedComposite = DecodedComposite()

        if (composite.partsList.size == 1 && composite.partsList.first().elementCase == Part.ElementCase.PART) {
            decodedComposite.encodedContent = composite.partsList.first().part
            return decodedComposite
        }
        decodedComposite.parts = composite.partsList.map { fromCompositePart(part = it) }
        return decodedComposite
    }

    private fun fromCompositePart(part: Part): DecodedComposite {
        return when (part.elementCase) {
            Part.ElementCase.PART -> {
                DecodedComposite(emptyList(), part.part)
            }
            Part.ElementCase.COMPOSITE -> {
                DecodedComposite(part.composite.partsList.map { fromCompositePart(it) })
            }
            else -> DecodedComposite()
        }
    }
}
