package trailbase

import (
	"bytes"

	"net/http"
	"net/url"
)

type thinClient struct {
	base   *url.URL
	client *http.Client
}

func (c *thinClient) do(method string, path string, headers []Header, body []byte, queryParams []QueryParam) (*http.Response, error) {
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
	return c.client.Do(req)
}

func (c *thinClient) get(url string) (*http.Response, error) {
	return c.client.Get(url)
}
