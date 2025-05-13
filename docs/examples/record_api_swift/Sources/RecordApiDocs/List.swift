import Foundation
import TrailBase

func list(client: Client) async throws -> ListResponse<SimpleStrict> {
  try await client
    .records("movies")
    .list(
      pagination: Pagination(limit: 3),
      order: ["rank"],
      filters: ["watch_time[lt]=120", "description[like]=%love%"]
    )
}
