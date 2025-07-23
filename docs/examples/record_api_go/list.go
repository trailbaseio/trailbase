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
			trailbase.FilterColumn{Column: "watch_time", Op: trailbase.LessThan, Value: "120"},
			trailbase.FilterColumn{Column: "description", Op: trailbase.Like, Value: "%love%"},
		},
	})
}
