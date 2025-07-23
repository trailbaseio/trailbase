package record_api_docs

import "github.com/trailbaseio/trailbase/client/go/trailbase"

func Read(client trailbase.Client, id trailbase.RecordId) (*SimpleStrict, error) {
	api := trailbase.NewRecordApi[SimpleStrict](client, "simple_strict_table")
	return api.Read(id)
}
