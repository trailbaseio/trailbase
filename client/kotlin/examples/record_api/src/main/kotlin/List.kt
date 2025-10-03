package io.trailbase.examples.recordApi

import io.trailbase.client.*
import kotlinx.serialization.json.JsonObject

suspend fun list(client: Client): ListResponse<JsonObject> {
    return client
        .records("movies")
        .list(
            pagination = Pagination(limit = 3),
            order = listOf("rank"),
            filters =
                listOf(
                    Filter(column = "watch_time", op = CompareOp.lessThan, value = "120"),
                    Filter(column = "description", op = CompareOp.like, value = "%love%"),
                ),
        )
}
