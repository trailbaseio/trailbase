package record_api_docs

import "github.com/trailbaseio/trailbase/client/go/trailbase"

func Delete(client trailbase.Client, id trailbase.RecordId) error {
	api := trailbase.NewRecordApi[SimpleStrict](client, "simple_strict_table")
	return api.Delete(id)
}
