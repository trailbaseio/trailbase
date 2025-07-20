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

	api := client.RecordApi("simple_strict_table")
	id, err := api.Create(SimpleStrict{
		TextNotNull: "test",
	})
	if err != nil {
		t.Fatal(err)
	}

	_ = id
}
