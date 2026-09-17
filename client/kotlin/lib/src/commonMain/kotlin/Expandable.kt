package io.trailbase.client

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.descriptors.buildClassSerialDescriptor
import kotlinx.serialization.descriptors.element
import kotlinx.serialization.encoding.CompositeDecoder
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.encoding.decodeStructure

/**
 * Wrapper type for representing conditionally expandable columns, i.e. foreign-key columns that
 * reference other records. When the APIs are configured to allow expansion, reads and listings can
 * conditionally inline the data of the referenced record. Otherwise, only the foreign-key is
 * present. The wrapper allows to use the same bindings for both, simple and expanded, read
 * operations as well as mutations. For mutations use `Expandable.id()`.
 */
@ConsistentCopyVisibility
@Serializable(with = ExpandableSerializer::class)
data class Expandable<T>
    private constructor(
        val id: RecordId,
        val data: T? = null,
    ) {
        companion object {
            fun <T> id(id: RecordId) = Expandable<T>(id)

            fun <T> withData(
                id: RecordId,
                data: T?,
            ) = Expandable(id, data)
        }
    }

/**
 * Custom serializer for [Expandable].
 *
 * Deserialization is virtually vanilla, however serialization omits any data, serializing only the
 * id field.
 *
 * https://github.com/Kotlin/kotlinx.serialization/blob/master/docs/serializers.md#handwritten-composite-serializer
 */
open class ExpandableSerializer<T>(
    private val serializer: KSerializer<T>,
) : KSerializer<Expandable<T>> {
    override val descriptor: SerialDescriptor =
        buildClassSerialDescriptor(Expandable::class.simpleName!!) {
            element<String>("id")
            element("data", serializer.descriptor)
        }

    override fun serialize(
        encoder: Encoder,
        value: Expandable<T>,
    ) {
        encoder.encodeString(value.id.id())
    }

    override fun deserialize(decoder: Decoder): Expandable<T> =
        decoder.decodeStructure(descriptor) {
            var id: String? = null
            var data: T? = null
            while (true) {
                when (val index = decodeElementIndex(descriptor)) {
                    0 -> id = decodeStringElement(descriptor, index)
                    1 -> data = decodeSerializableElement(descriptor, index, serializer)
                    CompositeDecoder.DECODE_DONE -> break
                }
            }
            if (id == null) {
                throw SerializationException(
                    "Failed to parse Expandable: missing id. Maybe not an FK column or configured as `expand` in the API?",
                )
            }

            Expandable.withData(RecordId.parse(id), data)
        }
}
