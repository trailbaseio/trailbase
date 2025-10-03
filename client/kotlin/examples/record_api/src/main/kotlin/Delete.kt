package io.trailbase.examples.recordApi

import io.trailbase.client.*

suspend fun delete(client: Client, id: RecordId) {
    return client.records("simple_strict_table").delete(id)
}
