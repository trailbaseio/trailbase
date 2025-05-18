import Foundation
import TrailBase

func list(client: Client) async throws -> ListResponse<Movie> {
    try await client
        .records("movies")
        .list(
            pagination: Pagination(limit: 3),
            order: ["rank"],
            filters: [
                .Filter(column: "watch_time", op: .LessThan, value: "120"),
                .Filter(column: "description", op: .Like, value: "%love%"),
            ]
        )
}
