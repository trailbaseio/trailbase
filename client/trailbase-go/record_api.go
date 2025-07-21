package trailbase

import (
	"errors"
	"fmt"
	"io"

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

func NewRecordApi[T any](c Client, name string) *RecordApi[T] {
	return &RecordApi[T]{
		client: c,
		name:   name,
	}
}

const recordApi string = "api/records/v1"
