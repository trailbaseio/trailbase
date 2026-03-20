package trailbase

import (
	"bytes"

	"net/http"
	"net/url"
)

type Transport interface {
	BaseUrl() *url.URL
	// Similar to `http.Client.Do`.
	Do(method string, path string, headers []Header, body []byte, queryParams []QueryParam) (*http.Response, error)
	// Convenience short-cut.
	Get(url string) (*http.Response, error)
}

type defaultTransport struct {
	base   *url.URL
	client *http.Client
}

func (c *defaultTransport) BaseUrl() *url.URL {
	return c.base
}

func (c *defaultTransport) Get(url string) (*http.Response, error) {
	return c.client.Get(url)
}

func (c *defaultTransport) Do(method string, path string, headers []Header, body []byte, queryParams []QueryParam) (*http.Response, error) {
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
