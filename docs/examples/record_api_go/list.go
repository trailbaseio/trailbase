package record_api_docs

import "github.com/trailbaseio/trailbase/client/go/trailbase"

func List(client trailbase.Client) (*trailbase.ListResponse[Movie], error) {
	api := trailbase.NewRecordApi[Movie](client, "movies")

	limit := uint64(3)

	return api.List(&trailbase.ListArguments{
		Pagination: trailbase.Pagination{
			Limit: &limit,
		},
		Order: []string{"rank"},
		Filters: []trailbase.Filter{
			// Multiple filters on same column: watch_time between 90 and 120 minutes
			trailbase.FilterColumn{Column: "watch_time", Op: trailbase.GreaterThanOrEqual, Value: "90"},
			trailbase.FilterColumn{Column: "watch_time", Op: trailbase.LessThan, Value: "120"},
			// Date range: movies released between 2020 and 2023
			trailbase.FilterColumn{Column: "release_date", Op: trailbase.GreaterThanOrEqual, Value: "2020-01-01"},
			trailbase.FilterColumn{Column: "release_date", Op: trailbase.LessThanOrEqual, Value: "2023-12-31"},
		},
	})
}
