package trailbase

import (
	"errors"
	"fmt"
	"io"

	"encoding/json"
	"net/http"
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

type internalClient interface {
	do(method string, path string, body []byte, queryParams []QueryParam) (*http.Response, error)
}

type RecordApi struct {
	client internalClient
	name   string
}

func (r *RecordApi) Create(record any) (RecordId, error) {
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

func (r *RecordApi) Read(id RecordId, v any) error {
	resp, err := r.client.do("GET", fmt.Sprintf("%s/%s/%s", recordApi, r.name, id.ToString()), []byte{}, []QueryParam{})
	if err != nil {
		return err
	}
	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return err
	}

	return json.Unmarshal(respBody, v)
}

const recordApi string = "api/records/v1"
