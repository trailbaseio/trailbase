package trailbase

import (
	"bytes"
	"fmt"
	"io"

	"net/http"
	"net/url"
)

type FetchError struct {
	StatusCode int
	Message    string
	URL        *url.URL
}

func (e *FetchError) Error() string {
	if e.URL != nil {
		return fmt.Sprintf("FetchError(%d: %s, %s)", e.StatusCode, e.Message, e.URL)
	}
	return fmt.Sprintf("FetchError(%d: %s)", e.StatusCode, e.Message)
}

type thinClient struct {
	base   *url.URL
	client *http.Client
}

func (c *thinClient) do(method string, path string, headers []Header, body []byte, queryParams []QueryParam, errOnFail bool) (*http.Response, error) {
	req, err := http.NewRequest(method, c.base.JoinPath(path).String(), bytes.NewBuffer(body))
	if err != nil {
		return nil, err
	}
	for _, header := range headers {
		req.Header.Add(header.key, header.value)
	}
	if len(queryParams) > 0 {
		query := req.URL.Query()
		for _, param := range queryParams {
			query.Add(param.key, param.value)
		}
		req.URL.RawQuery = query.Encode()
	}
	resp, err := c.client.Do(req)
	if err != nil {
		return nil, err
	}

	if resp.StatusCode >= 400 && errOnFail {
		respBody, err := io.ReadAll(resp.Body)
		if err != nil {
			return nil, err
		}
		return nil, &FetchError{StatusCode: resp.StatusCode, Message: string(respBody), URL: req.URL}
	}

	return resp, err
}

func (c *thinClient) get(url string) (*http.Response, error) {
	return c.client.Get(url)
}
