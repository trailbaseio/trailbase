package trailbase

import (
	"errors"
	"fmt"
	"io"
	"strings"

	"encoding/json"
)

type RecordId interface {
	ToString() string
}

type IntRecordId int64

func (id IntRecordId) ToString() string {
	return fmt.Sprint(id)
}

type StringRecordId string

func (id StringRecordId) ToString() string {
	return string(id)
}

type RecordIdResponse struct {
	Ids []string `json:"ids"`
}

type ListResponse[T any] struct {
	Records    []T     `json:"records"`
	Cursor     *string `json:"cursor,omitempty"`
	TotalCount *int64  `json:"total_count,omitempty"`
}

type RecordApi[T any] struct {
	client Client
	name   string
}

func (r *RecordApi[T]) Create(record T) (RecordId, error) {
	reqBody, err := json.Marshal(record)
	if err != nil {
		return nil, err
	}

	resp, err := r.client.do("POST", fmt.Sprintf("%s/%s", recordApi, r.name), reqBody, []QueryParam{})
	if err != nil {
		return nil, err
	}
	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}

	var recordIdResponse RecordIdResponse
	err = json.Unmarshal(respBody, &recordIdResponse)
	if err != nil {
		return nil, err
	}

	if len(recordIdResponse.Ids) != 1 {
		return nil, errors.New("expected one id")
	}
	return StringRecordId(recordIdResponse.Ids[0]), nil
}

func (r *RecordApi[T]) Read(id RecordId) (*T, error) {
	resp, err := r.client.do("GET", fmt.Sprintf("%s/%s/%s", recordApi, r.name, id.ToString()), []byte{}, []QueryParam{})
	if err != nil {
		return nil, err
	}
	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}

	var value T
	err = json.Unmarshal(respBody, &value)
	if err != nil {
		return nil, err
	}
	return &value, nil
}

func (r *RecordApi[T]) Update(id RecordId, record T) error {
	reqBody, err := json.Marshal(record)
	if err != nil {
		return err
	}
	_, err = r.client.do("PATCH", fmt.Sprintf("%s/%s/%s", recordApi, r.name, id.ToString()), reqBody, []QueryParam{})
	if err != nil {
		return err
	}
	return nil
}

func (r *RecordApi[T]) Delete(id RecordId) error {
	_, err := r.client.do("DELETE", fmt.Sprintf("%s/%s/%s", recordApi, r.name, id.ToString()), []byte{}, []QueryParam{})
	if err != nil {
		return err
	}
	return nil
}

type Filter interface {
	toParams(path string) []QueryParam
}

type CompareOp int

const (
	Undefined CompareOp = iota
	Equal
	NotEqual
	LessThan
	LessThanEqual
	GreaterThan
	GreaterThanEqual
	Like
	Regex
)

func (op CompareOp) toString() string {
	switch op {
	case Equal:
		return "$eq"
	case NotEqual:
		return "$ne"
	case LessThan:
		return "$lt"
	case LessThanEqual:
		return "$lte"
	case GreaterThan:
		return "$gt"
	case GreaterThanEqual:
		return "$gte"
	case Like:
		return "$like"
	case Regex:
		return "$re"
	default:
		panic(fmt.Sprint("Unknown operation:", op))
	}
}

type FilterColumn struct {
	Column string
	Op     CompareOp
	Value  string
}

func (f FilterColumn) toParams(path string) []QueryParam {
	if f.Op != Undefined {
		return []QueryParam{
			QueryParam{
				key:   fmt.Sprintf("%s[%s][%s]", path, f.Column, f.Op.toString()),
				value: f.Value,
			},
		}
	}
	return []QueryParam{
		QueryParam{
			key:   fmt.Sprintf("%s[%s]", path, f.Column),
			value: f.Value,
		},
	}
}

type FilterAnd struct {
	filters []Filter
}

func (f FilterAnd) toParams(path string) []QueryParam {
	params := []QueryParam{}
	for i, nested := range f.filters {
		params = append(params, nested.toParams(fmt.Sprintf("%s[$and][%d]", path, i))...)
	}
	return params
}

type FilterOr struct {
	filters []Filter
}

func (f FilterOr) toParams(path string) []QueryParam {
	params := []QueryParam{}
	for i, nested := range f.filters {
		params = append(params, nested.toParams(fmt.Sprintf("%s[$or][%d]", path, i))...)
	}
	return params
}

type Pagination struct {
	Cursor *string
	Limit  *uint64
	Offset *uint64
}

type ListArguments struct {
	Order   []string
	Filters []Filter
	Expand  []string
	Count   bool

	Pagination
}

func (r *RecordApi[T]) List(args *ListArguments) (*ListResponse[T], error) {
	queryParams := []QueryParam{}

	if args != nil {
		if args.Cursor != nil && *args.Cursor != "" {
			queryParams = append(queryParams, QueryParam{
				key:   "cursor",
				value: *args.Cursor,
			})
		}
		if args.Limit != nil {
			queryParams = append(queryParams, QueryParam{
				key:   "limit",
				value: fmt.Sprint(*args.Limit),
			})
		}
		if args.Offset != nil {
			queryParams = append(queryParams, QueryParam{
				key:   "offset",
				value: fmt.Sprint(*args.Offset),
			})
		}
		if len(args.Order) > 0 {
			queryParams = append(queryParams, QueryParam{
				key:   "order",
				value: strings.Join(args.Order, ","),
			})
		}
		if len(args.Expand) > 0 {
			queryParams = append(queryParams, QueryParam{
				key:   "expand",
				value: strings.Join(args.Expand, ","),
			})
		}
		if args.Count {
			queryParams = append(queryParams, QueryParam{
				key:   "count",
				value: "true",
			})
		}
		for _, filter := range args.Filters {
			queryParams = append(queryParams, filter.toParams("filter")...)
		}
	}

	resp, err := r.client.do("GET", fmt.Sprintf("%s/%s", recordApi, r.name), []byte{}, queryParams)
	if err != nil {
		return nil, err
	}
	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}

	var listResponse ListResponse[T]
	err = json.Unmarshal(respBody, &listResponse)
	if err != nil {
		return nil, err
	}

	return &listResponse, nil
}

func NewRecordApi[T any](c Client, name string) *RecordApi[T] {
	return &RecordApi[T]{
		client: c,
		name:   name,
	}
}

const recordApi string = "api/records/v1"
