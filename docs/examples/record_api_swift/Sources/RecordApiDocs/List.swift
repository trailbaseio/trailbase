import Foundation
import TrailBase

func list(client: Client) async throws -> ListResponse<Movie> {
    try await client
        .records("movies")
        .list(
            pagination: Pagination(limit: 3),
            order: ["rank"],
            filters: [
                // Multiple filters on same column: watch_time between 90 and 120 minutes
                .Filter(column: "watch_time", op: .GreaterThanOrEqual, value: "90"),
                .Filter(column: "watch_time", op: .LessThan, value: "120"),
                // Date range: movies released between 2020 and 2023
                .Filter(column: "release_date", op: .GreaterThanOrEqual, value: "2020-01-01"),
                .Filter(column: "release_date", op: .LessThanOrEqual, value: "2023-12-31"),
            ]
        )
}
