package io.trailbase.examples.recordApi

import io.trailbase.client.*

suspend fun create(client: Client): RecordId {
    return client.records("simple_strict_table").create(SimpleStrictInsert(text_not_null = "test"))
}
