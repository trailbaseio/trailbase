package io.trailbase.examples.recordApi

import io.trailbase.client.*

suspend fun read(client: Client, id: RecordId): SimpleStrict {
    return client.records("simple_strict_table").read(id)
}
