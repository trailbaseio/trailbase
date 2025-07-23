package record_api_docs

import (
	"testing"

	"github.com/trailbaseio/trailbase/client/go/trailbase"
)

func connect() (trailbase.Client, error) {
	client, err := trailbase.NewClient("http://localhost:4000")
	if err != nil {
		return nil, err
	}
	_, err = client.Login("admin@localhost", "secret")
	if err != nil {
		return nil, err
	}
	return client, err
}

func TestDocs(t *testing.T) {
	client, err := connect()
	if err != nil {
		t.Fatal(err)
	}

	movies, err := List(client)
	if err != nil {
		t.Fatal(err)
	}
	_ = movies

	id, err := Create(client)
	if err != nil {
		t.Fatal(err)
	}
	err = Update(client, id)
	if err != nil {
		t.Fatal(err)
	}
	record, err := Read(client, id)
	if err != nil {
		t.Fatal(err)
	}

	if record.TextNotNull != "updated" {
		t.Fatal("expected 'updated', got", record.TextNotNull)
	}

	err = Delete(client, id)
	if err != nil {
		t.Fatal(err)
	}
}
