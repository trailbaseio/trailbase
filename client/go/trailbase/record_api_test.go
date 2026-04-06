package trailbase

import (
	"fmt"
	"testing"
)

func testEq[T comparable](a, b []T) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func TestFilter(t *testing.T) {
	got0 := FilterColumn{
		Column: "col",
		Value:  "value",
	}.toParams("filter")
	expected0 := []QueryParam{
		{key: "filter[col]", value: "value"},
	}
	if !testEq(got0, expected0) {
		t.Fatal("unexpected filter, got:", got0, " expected: ", expected0)
	}

	got1 := FilterAnd{
		filters: []Filter{
			FilterColumn{
				Column: "col0",
				Value:  "val0",
			},
			FilterOr{
				filters: []Filter{
					FilterColumn{
						Column: "col1",
						Op:     NotEqual,
						Value:  "val1",
					},
					FilterColumn{
						Column: "col2",
						Op:     LessThan,
						Value:  "val2",
					},
				},
			},
		},
	}.toParams("filter")
	expected1 := []QueryParam{
		{key: "filter[$and][0][col0]", value: "val0"},
		{key: "filter[$and][1][$or][0][col1][$ne]", value: "val1"},
		{key: "filter[$and][1][$or][1][col2][$lt]", value: "val2"},
	}

	if !testEq(got1, expected1) {
		t.Fatal("unexpected filter, got:", got1, " expected: ", expected1)
	}
}

func TestEventParsing(t *testing.T) {
	{
		errJson := `
	  {
      "Error": {
        "status": 1,
        "message": "test"
      },
      "seq": 3
    }`

		errEvent, err := parseEvent(fmt.Append([]byte("data: "), errJson))
		if err != nil {
			t.Fatal("Got err", err)
		}
		if errEvent == nil {
			t.Fatal("Expected event, got nil")
		}

		msg := errEvent.Error.Message
		if *msg != "test" {
			t.Fatal("Expected message is 'test'")
		}

		if *errEvent.Seq != 3 {
			t.Fatal("Expected Seq of 3, got:", errEvent.Seq)
		}
	}

	{
		errJson := `
	  {
      "Error": {
        "status": 1
      }
    }`

		errEvent, err := parseEvent(fmt.Append([]byte("data: "), errJson))
		if err != nil {
			t.Fatal("Got err", err)
		}
		if errEvent == nil {
			t.Fatal("Expected event, got nil")
		}

		msg := errEvent.Error.Message
		if msg != nil {
			t.Fatal("expected nil message, got ", msg)
		}
	}

	{
		updateJson := `
	  {
      "Update": {
        "col0": 5
      },
      "seq": 4
    }`

		updateEvent, err := parseEvent(fmt.Append([]byte("data: "), updateJson))
		if err != nil {
			t.Fatal("Got err", err)
		}
		if updateEvent == nil {
			t.Fatal("Expected event, got nil")
		}

		if updateEvent.Error != nil {
			t.Fatal("expected event got error", updateEvent.Error)
		}

		if *updateEvent.Seq != 4 {
			t.Fatal("expected Seq=4")
		}
		if updateEvent.Value == nil {
			t.Fatal("expected update value")
		}
	}
}
