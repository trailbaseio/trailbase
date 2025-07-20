package trailbase

import (
	"errors"
	"io"

	"encoding/json"
	"net/http"
)

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

func (r *RecordApi) Create(record any) (*string, error) {
	reqBody, err := json.Marshal(record)
	if err != nil {
		return nil, err
	}

	resp, err := r.client.do("POST", recordApi+"/"+r.name, reqBody, []QueryParam{})
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
	return &recordIdResponse.Ids[0], nil
}
