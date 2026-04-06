package trailbase

import (
	"bufio"
	"bytes"
	"errors"
	"fmt"
	"io"
	"strings"
	"sync"
	"time"

	"encoding/base64"
	"encoding/json"
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

type User struct {
	Sub   string
	Email string
}

type Tokens struct {
	AuthToken    string  `json:"auth_token"`
	RefreshToken *string `json:"refresh_token,omitempty"`
	CsrfToken    *string `json:"csrf_token,omitempty"`
}

type MultiFactorAuthToken struct {
	Token string `json:"mfa_token"`
}

type JwtTokenClaims struct {
	Sub       string `json:"sub"`
	Iat       int64  `json:"iat"`
	Exp       int64  `json:"exp"`
	Email     string `json:"email"`
	CsrfToken string `json:"csrf_token"`
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

	claims, err := decodeJwtTokenClaims(tokens.AuthToken)
	if err != nil {
		return nil, err
	}

	return &TokenState{
		s: &state{
			tokens: *tokens,
			claims: *claims,
		},
		headers: buildHeaders(tokens),
	}, nil
}

func NewClient(baseUrl string) (*Client, error) {
	return NewClientWithTokens(baseUrl, nil)
}

func NewClientWithTokens(baseUrl string, tokens *Tokens) (*Client, error) {
	base, err := url.Parse(baseUrl)
	if err != nil {
		return nil, err
	}
	tokenState, err := NewTokenState(tokens)
	if err != nil {
		return nil, err
	}
	return &Client{
		client: &defaultTransport{
			base:   base,
			client: &http.Client{},
		},
		tokenState: tokenState,
		tokenMutex: &sync.Mutex{},
	}, nil
}

type Client struct {
	client Transport

	tokenState *TokenState
	tokenMutex *sync.Mutex
}

func (c *Client) BaseUrl() *url.URL {
	return c.client.BaseUrl()
}

func (c *Client) Tokens() *Tokens {
	c.tokenMutex.Lock()
	defer c.tokenMutex.Unlock()
	if c.tokenState != nil && c.tokenState.s != nil {
		return &c.tokenState.s.tokens
	}
	return nil
}

func (c *Client) User() *User {
	c.tokenMutex.Lock()
	defer c.tokenMutex.Unlock()
	if c.tokenState != nil && c.tokenState.s != nil {
		claims := c.tokenState.s.claims
		sub := claims.Sub
		email := claims.Email

		return &User{
			Sub:   sub,
			Email: email,
		}
	}
	return nil
}

func (c *Client) Login(email string, password string) (*MultiFactorAuthToken, error) {
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

	resp, err := c.do("POST", authApi+"/login", reqBody, nil)
	if err != nil {
		ferr, ok := err.(*FetchError)
		if ok && ferr != nil && ferr.StatusCode == 403 {
			var mfaToken MultiFactorAuthToken
			err = json.Unmarshal([]byte(ferr.Message), &mfaToken)
			if err != nil {
				return nil, err
			}

			return &mfaToken, nil
		}

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

	c.updateTokens(&tokens)

	return nil, nil
}

func (c *Client) LoginSecond(token *MultiFactorAuthToken, code string) error {
	type Credentials struct {
		Token    string `json:"mfa_token"`
		TotpCode string `json:"totp"`
	}

	reqBody, err := json.Marshal(Credentials{
		Token:    token.Token,
		TotpCode: code,
	})
	if err != nil {
		return err
	}

	resp, err := c.do("POST", authApi+"/login_mfa", reqBody, nil)
	if err != nil {
		return err
	}

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return err
	}

	var tokens Tokens
	err = json.Unmarshal(respBody, &tokens)
	if err != nil {
		return err
	}

	c.updateTokens(&tokens)

	return nil
}

func (c *Client) RequestOtp(email string, redirectUri *string) error {
	type Request struct {
		Email       string  `json:"email"`
		RedirectUri *string `json:"redirect_uri,omitempty"`
	}

	reqBody, err := json.Marshal(Request{
		Email:       email,
		RedirectUri: redirectUri,
	})
	if err != nil {
		return err
	}

	resp, err := c.do("POST", authApi+"/otp/request", reqBody, nil)
	if err != nil {
		return err
	}
	_ = resp

	return nil
}

func (c *Client) LoginOtp(email string, code string) error {
	type Request struct {
		Email string `json:"email"`
		Code  string `json:"code"`
	}

	reqBody, err := json.Marshal(Request{
		Email: email,
		Code:  code,
	})
	if err != nil {
		return err
	}

	resp, err := c.do("POST", authApi+"/otp/login", reqBody, nil)
	if err != nil {
		return err
	}

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return err
	}

	var tokens Tokens
	err = json.Unmarshal(respBody, &tokens)
	if err != nil {
		return err
	}
	c.updateTokens(&tokens)

	return nil
}

func (c *Client) Logout() error {
	url := c.BaseUrl().JoinPath(authApi, "logout").String()
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

		_, err = c.do("POST", authApi+"/logout", body, nil)
		if err != nil {
			return err
		}
	} else {
		_, err := c.client.Get(url)
		if err != nil {
			return err
		}
	}

	_, err := c.updateTokens(nil)
	return err
}

func (c *Client) Refresh() error {
	headerAndRefresh := c.getHeadersAndRefreshToken()
	if headerAndRefresh == nil {
		return errors.New("Unauthenticated")
	}

	newTokenState, err := doRefreshToken(c.client, headerAndRefresh.headers, headerAndRefresh.refreshToken)
	if err != nil {
		return err
	}

	c.tokenMutex.Lock()
	defer c.tokenMutex.Unlock()
	c.tokenState = newTokenState

	return nil
}

func (c *Client) do(method string, path string, body []byte, queryParams []QueryParam) (*http.Response, error) {
	headers, refreshToken := c.getHeadersAndRefreshTokenIfExpired()
	if refreshToken != nil {
		newTokenState, err := doRefreshToken(c.client, headers, *refreshToken)
		if err != nil {
			return nil, err
		}
		headers = newTokenState.headers
		c.tokenMutex.Lock()
		defer c.tokenMutex.Unlock()

		c.tokenState = newTokenState
	}

	resp, err := c.client.Do(method, path, headers, body, queryParams)
	if err != nil {
		return nil, err
	}

	if resp.StatusCode >= 400 {
		respBody, err := io.ReadAll(resp.Body)
		if err != nil {
			return nil, err
		}
		return nil, &FetchError{StatusCode: resp.StatusCode, Message: string(respBody), URL: c.BaseUrl().JoinPath(path)}
	}

	return resp, nil
}

func (c *Client) stream(method string, path string, body []byte, queryParams []QueryParam) (<-chan Event, func(), error) {
	resp, err := c.do(method, path, body, queryParams)
	if err != nil {
		return nil, nil, err
	}

	scanner := bufio.NewScanner(resp.Body)
	scanner.Split(sseSplitter)

	stream := make(chan Event)

	go func() {
		defer close(stream)

		for scanner.Scan() {
			event, err := parseEvent(scanner.Bytes())
			if err != nil {
				return
			}

			if event != nil {
				stream <- *event
			}
		}
	}()

	return stream, func() {
		resp.Body.Close()
	}, nil
}

func (c *Client) updateTokens(tokens *Tokens) (*Tokens, error) {
	state, err := NewTokenState(tokens)
	if err != nil {
		return nil, err
	}

	c.tokenMutex.Lock()
	defer c.tokenMutex.Unlock()
	c.tokenState = state

	return tokens, nil
}

type HeadersAndRefreshToken struct {
	headers      []Header
	refreshToken string
}

func (c *Client) getHeadersAndRefreshToken() *HeadersAndRefreshToken {
	var r *HeadersAndRefreshToken

	c.tokenMutex.Lock()
	defer c.tokenMutex.Unlock()

	s := c.tokenState
	if s != nil && s.s != nil && s.s.tokens.RefreshToken != nil {
		r = &HeadersAndRefreshToken{
			headers:      c.tokenState.headers,
			refreshToken: *c.tokenState.s.tokens.RefreshToken,
		}
	}

	return r
}

func (c *Client) getHeadersAndRefreshTokenIfExpired() ([]Header, *string) {
	shouldRefresh := func(exp int64) bool {
		now := time.Now()
		return exp-60 < now.Unix()
	}

	c.tokenMutex.Lock()
	defer c.tokenMutex.Unlock()

	s := c.tokenState
	if s == nil {
		return []Header{}, nil
	}

	headers := s.headers
	var refreshToken *string

	if s.s != nil && s.s.tokens.RefreshToken != nil {
		if shouldRefresh(s.s.claims.Exp) {
			refreshToken = s.s.tokens.RefreshToken
		}
	}

	return headers, refreshToken
}

func doRefreshToken(client Transport, headers []Header, refreshToken string) (*TokenState, error) {
	type RefreshRequest struct {
		RefreshToken string `json:"refresh_token"`
	}
	reqBody, err := json.Marshal(RefreshRequest{
		RefreshToken: refreshToken,
	})
	if err != nil {
		return nil, err
	}

	path := authApi + "/refresh"
	resp, err := client.Do("POST", path, headers, reqBody, nil)
	if err != nil {
		return nil, err
	}

	if resp.StatusCode >= 400 {
		respBody, err := io.ReadAll(resp.Body)
		if err != nil {
			return nil, err
		}
		return nil, &FetchError{StatusCode: resp.StatusCode, Message: string(respBody), URL: client.BaseUrl().JoinPath(path)}
	}

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}

	type RefreshResponse struct {
		AuthToken string  `json:"auth_token"`
		CsrfToken *string `json:"csrf_token,omitempty"`
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

func decodeJwtTokenClaims(jwt string) (*JwtTokenClaims, error) {
	parts := strings.Split(jwt, ".")
	if len(parts) != 3 {
		return nil, errors.New("Invalid JWT format")
	}

	data, err := base64.RawURLEncoding.DecodeString(parts[1])
	if err != nil {
		return nil, err
	}

	var jwtTokenClaims JwtTokenClaims
	err = json.Unmarshal(data, &jwtTokenClaims)
	if err != nil {
		return nil, err
	}
	return &jwtTokenClaims, nil
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

func sseSplitter(data []byte, atEOF bool) (advance int, token []byte, err error) {
	if atEOF && len(data) == 0 {
		return 0, nil, nil
	}
	if i := bytes.Index(data, []byte("\n\n")); i >= 0 {
		return i + 2, data[0:i], nil
	}
	if i := bytes.Index(data, []byte("\n")); i >= 0 {
		return i + 1, data[0:i], nil
	}
	if atEOF {
		return len(data), data, nil
	}
	return 0, nil, nil
}

var jsonHeader Header = Header{key: "Content-Type", value: "application/json"}

const authApi string = "api/auth/v1"
