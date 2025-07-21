package trailbase

import (
	"testing"
)

func newClient(t *testing.T) Client {
	client, err := NewClient("http://localhost:4000")
	if err != nil {
		panic(err)
	}
	tokens, err := client.Login("admin@localhost", "secret")
	if err != nil {
		t.Fatal(err)
	}
	if tokens == nil {
		t.Fatal("Missing tokens")
	}
	return client
}

func TestAuth(t *testing.T) {
	client := newClient(t)

	client.Refresh()

	err := client.Logout()
	if err != nil {
		t.Fatal(err)
	}
}

type SimpleStrict struct {
	Id *string `json:"id,omitempty"`

	TextNull    *string `json:"text_null,omitempty"`
	TextDefault *string `json:"text_default,omitempty"`
	TextNotNull string  `json:"text_not_null"`
}

func TestRecordApi(t *testing.T) {
	client := newClient(t)

	api := NewRecordApi[SimpleStrict](client, "simple_strict_table")
	id, err := api.Create(SimpleStrict{
		TextNotNull: "test",
	})
	if err != nil {
		t.Fatal(err)
	}

	simpleStrict0, err := api.Read(id)
	if err != nil {
		t.Fatal(err)
	}
	if simpleStrict0 == nil || simpleStrict0.TextNotNull != "test" {
		t.Fatal("expected 'test', got", simpleStrict0)
	}

	err = api.Update(id, SimpleStrict{
		TextNotNull: "test_updated",
	})
	if err != nil {
		t.Fatal(err)
	}

	simpleStrict1, err := api.Read(id)
	if err != nil {
		t.Fatal(err)
	}
	if simpleStrict1 == nil || simpleStrict1.TextNotNull != "test_updated" {
		t.Fatal("expected 'test_updated', got", simpleStrict0)
	}

	listResponse, err := api.List()
	if err != nil {
		t.Fatal(err)
	}
	if len(listResponse.Records) < 1 {
		t.Fatal("expected a record, got: ", listResponse.Records)
	}

	err = api.Delete(id)
	if err != nil {
		t.Fatal(err)
	}

	_, err = api.Read(id)
	if err == nil {
		t.Fatal("expected error reading delete record")
	}
}
