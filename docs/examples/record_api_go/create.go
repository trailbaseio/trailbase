package record_api_docs

import "github.com/trailbaseio/trailbase/client/go/trailbase"

func Create(client trailbase.Client) (trailbase.RecordId, error) {
	api := trailbase.NewRecordApi[SimpleStrict](client, "simple_strict_table")
	return api.Create(SimpleStrict{
		TextNotNull: "test",
	})
}
