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
	got0 := Filter{
		column: "col",
		value:  "value",
	}.toParams("filter")
	expected0 := []QueryParam{
		QueryParam{key: "filter[col]", value: "value"},
	}
	if !testEq(got0, expected0) {
		t.Fatal("unexpected filter, got:", got0, " expected: ", expected0)
	}

	got1 := FilterAnd{
		filters: []filter{
			Filter{
				column: "col0",
				value:  "val0",
			},
			FilterOr{
				filters: []filter{
					Filter{
						column: "col1",
						op:     NotEqual,
						value:  "val1",
					},
					Filter{
						column: "col2",
						op:     LessThan,
						value:  "val2",
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
