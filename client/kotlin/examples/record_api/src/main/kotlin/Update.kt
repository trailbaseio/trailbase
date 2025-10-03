package io.trailbase.examples.recordApi

import io.trailbase.client.*

suspend fun update(client: Client, id: RecordId, record: SimpleStrictUpdate) {
    return client.records("simple_strict_table").update(id, record)
}
