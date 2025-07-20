package main

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"sync"
	"time"

	"encoding/json"
	"net/http"
	"net/url"

	"github.com/golang-jwt/jwt/v5"
)

type User struct {
	Sub   string
	Email string
}

type Tokens struct {
	AuthToken    string  `json:"auth_token"`
	RefreshToken *string `json:"refresh_token"`
	CsrfToken    *string `json:"csrf_token"`
}

type state struct {
	tokens Tokens
	claims JwtTokenClaims
}

type Header struct {
	key   string
	value string
}

type QueryParam struct {
	key   string
	value string
}

type TokenState struct {
	s       *state
	headers []Header
}

func NewTokenState(tokens *Tokens) (*TokenState, error) {
	if tokens == nil {
		return &TokenState{
			s:       nil,
			headers: buildHeaders(tokens),
		}, nil
	}

	var claims JwtTokenClaims

	parser := jwt.NewParser()
	t, parts, err := parser.ParseUnverified(tokens.AuthToken, &claims)
	if err != nil {
		return nil, err
	}

	_ = t
	_ = parts

	return &TokenState{
		s: &state{
			tokens: *tokens,
			claims: claims,
		},
		headers: buildHeaders(tokens),
	}, nil
}

func buildHeaders(tokens *Tokens) []Header {
	headers := []Header{jsonHeader}

	if tokens != nil {
		headers = append(headers, Header{
			key:   "Authorization",
			value: "Bearer " + tokens.AuthToken,
		})

		if tokens.RefreshToken != nil {
			headers = append(headers, Header{
				key:   "Refresh-Token",
				value: *tokens.RefreshToken,
			})
		}

		if tokens.CsrfToken != nil {
			headers = append(headers, Header{
				key:   "CSRF-Token",
				value: *tokens.CsrfToken,
			})
		}
	}

	return headers
}

type JwtTokenClaims struct {
	Email     string `json:"email"`
	CsrfToken string `json:"csrf_token"`
	jwt.RegisteredClaims
}

type RecordIdResponse struct {
	Ids []string `json:"ids"`
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

type Client interface {
	Refresh() error
	Login(email string, password string) (*Tokens, error)
	Logout() error
	RecordApi(name string) *RecordApi
}

type internalClient interface {
	do(method string, path string, body []byte, queryParams []QueryParam) (*http.Response, error)
}

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
	for _, param := range queryParams {
		req.URL.Query().Add(param.key, param.value)
	}
	return c.client.Do(req)
}

func (c *thinClient) get(url string) (*http.Response, error) {
	return c.client.Get(url)
}

type ClientImpl struct {
	base   *url.URL
	client *thinClient

	tokenState *TokenState
	tokenMutex *sync.Mutex
}

func (c *ClientImpl) Refresh() error {
	headerAndRefresh := c.getHeadersAndRefreshToken()
	if headerAndRefresh == nil {
		return errors.New("Unauthenticated")
	}

	newTokenState, err := doRefreshToken(c.client, headerAndRefresh.headers, headerAndRefresh.refreshToken)
	if err != nil {
		return err
	}

	c.tokenMutex.Lock()
	c.tokenState = newTokenState
	c.tokenMutex.Unlock()

	return nil
}

func (c *ClientImpl) Login(email string, password string) (*Tokens, error) {
	type Credentials struct {
		Email    string `json:"email"`
		Password string `json:"password"`
	}

	reqBody, err := json.Marshal(Credentials{
		Email:    email,
		Password: password,
	})
	if err != nil {
		return nil, err
	}

	resp, err := c.client.do("POST", authApi+"/login", []Header{jsonHeader}, reqBody, []QueryParam{})
	if err != nil {
		return nil, err
	}

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}

	var tokens Tokens
	err = json.Unmarshal(respBody, &tokens)
	if err != nil {
		return nil, err
	}

	return c.updateTokens(&tokens)
}

func (c *ClientImpl) Logout() error {
	url := c.base.JoinPath(authApi, "logout").String()
	r := c.getHeadersAndRefreshToken()
	if r != nil {
		type LogoutRequest struct {
			RefreshToken string `json:"refresh_token"`
		}

		body, err := json.Marshal(LogoutRequest{
			RefreshToken: r.refreshToken,
		})
		if err != nil {
			return err
		}

		_, err = c.client.do("POST", authApi+"/logout", []Header{jsonHeader}, body, []QueryParam{})
		if err != nil {
			return err
		}
	} else {
		_, err := c.client.get(url)
		if err != nil {
			return err
		}
	}

	_, err := c.updateTokens(nil)
	return err
}

func (c *ClientImpl) RecordApi(name string) *RecordApi {
	return &RecordApi{
		client: c,
		name:   name,
	}
}

func (c *ClientImpl) do(method string, path string, body []byte, queryParams []QueryParam) (*http.Response, error) {
	headers, refreshToken := c.getHeadersAndRefreshTokenIfExpired()
	if refreshToken != nil {
		newTokenState, err := doRefreshToken(c.client, headers, *refreshToken)
		if err != nil {
			return nil, err
		}
		headers = newTokenState.headers
		c.tokenMutex.Lock()
		c.tokenState = newTokenState
		c.tokenMutex.Unlock()
	}

	return c.client.do(method, path, headers, body, queryParams)
}

func (c *ClientImpl) updateTokens(tokens *Tokens) (*Tokens, error) {
	state, err := NewTokenState(tokens)
	if err != nil {
		return nil, err
	}

	c.tokenMutex.Lock()
	c.tokenState = state
	c.tokenMutex.Unlock()

	return tokens, nil
}

type HeadersAndRefreshToken struct {
	headers      []Header
	refreshToken string
}

func (c *ClientImpl) getHeadersAndRefreshToken() *HeadersAndRefreshToken {
	var r *HeadersAndRefreshToken

	c.tokenMutex.Lock()
	s := c.tokenState
	if s != nil && s.s != nil && s.s.tokens.RefreshToken != nil {
		r = &HeadersAndRefreshToken{
			headers:      c.tokenState.headers,
			refreshToken: *c.tokenState.s.tokens.RefreshToken,
		}
	}
	c.tokenMutex.Unlock()

	return r
}

func (c *ClientImpl) getHeadersAndRefreshTokenIfExpired() ([]Header, *string) {
	shouldRefresh := func(exp int64) bool {
		now := time.Now()
		return exp-60 < now.Unix()
	}

	c.tokenMutex.Lock()

	s := c.tokenState
	if s == nil {
		c.tokenMutex.Unlock()
		return []Header{}, nil
	}

	headers := s.headers
	var refreshToken string

	if s.s != nil && s.s.tokens.RefreshToken != nil {
		exp := s.s.claims.ExpiresAt
		if exp != nil && shouldRefresh(exp.Unix()) {
			refreshToken = *s.s.tokens.RefreshToken
		}
	}
	c.tokenMutex.Unlock()

	return headers, &refreshToken
}

func doRefreshToken(client *thinClient, headers []Header, refreshToken string) (*TokenState, error) {
	type RefreshRequest struct {
		RefreshToken string `json:"refresh_token"`
	}
	type RefreshResponse struct {
		AuthToken string  `json:"auth_token"`
		CsrfToken *string `json:"csrf_token"`
	}

	reqBody, err := json.Marshal(RefreshRequest{
		RefreshToken: refreshToken,
	})
	if err != nil {
		return nil, err
	}

	resp, err := client.do("POST", authApi+"/refresh", headers, reqBody, []QueryParam{})
	if err != nil {
		return nil, err
	}

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}

	var refreshResp RefreshResponse
	err = json.Unmarshal(respBody, &refreshResp)
	if err != nil {
		return nil, err
	}

	return NewTokenState(&Tokens{
		AuthToken:    refreshResp.AuthToken,
		RefreshToken: &refreshToken,
		CsrfToken:    refreshResp.CsrfToken,
	})
}

func NewClient(site string) (Client, error) {
	base, err := url.Parse(site)
	if err != nil {
		return nil, err
	}
	return &ClientImpl{
		base: base,
		client: &thinClient{
			base:   base,
			client: &http.Client{},
		},
		tokenState: nil,
		tokenMutex: &sync.Mutex{},
	}, nil
}

func main() {
	client, err := NewClient("http://localhost:4000")
	if err != nil {
		panic(err)
	}
	tokens, err := client.Login("admin@localhost", "secret")
	if err != nil {
		panic(err)
	}

	fmt.Println("Tokens: ", tokens)

	client.Refresh()

	type SimpleStrict struct {
		Id *string `json:"id"`

		TextNull    *string `json:"text_null"`
		TextDefault *string `json:"text_default"`
		TextNotNull string  `json:"text_not_null"`
	}

	api := client.RecordApi("simple_strict_table")
	api.Create(SimpleStrict{
		TextNotNull: "test",
	})

	err = client.Logout()
	if err != nil {
		panic(err)
	}
}

var jsonHeader Header = Header{key: "Content-Type", value: "application/json"}

const authApi string = "api/auth/v1"
const recordApi string = "api/records/v1"
