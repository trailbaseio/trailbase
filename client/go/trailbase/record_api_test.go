package trailbase

import (
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
		QueryParam{key: "filter[col]", value: "value"},
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
		QueryParam{key: "filter[$and][0][col0]", value: "val0"},
		QueryParam{key: "filter[$and][1][$or][0][col1][$ne]", value: "val1"},
		QueryParam{key: "filter[$and][1][$or][1][col2][$lt]", value: "val2"},
	}

	if !testEq(got1, expected1) {
		t.Fatal("unexpected filter, got:", got1, " expected: ", expected1)
	}
}
