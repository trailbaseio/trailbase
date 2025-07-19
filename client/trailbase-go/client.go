package trailbase

import (
	"errors"
	"io"
	"strings"
	"sync"
	"time"

	"encoding/base64"
	"encoding/json"
	"net/http"
	"net/url"
)

type User struct {
	Sub   string
	Email string
}

type Tokens struct {
	AuthToken    string  `json:"auth_token"`
	RefreshToken *string `json:"refresh_token,omitempty"`
	CsrfToken    *string `json:"csrf_token,omitempty"`
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

type Client interface {
	Site() *url.URL
	Tokens() *Tokens
	User() *User

	// Authenticate
	Login(email string, password string) (*Tokens, error)
	Logout() error
	Refresh() error

	// Internal
	do(method string, path string, body []byte, queryParams []QueryParam) (*http.Response, error)
}

type ClientImpl struct {
	base   *url.URL
	client *thinClient

	tokenState *TokenState
	tokenMutex *sync.Mutex
}

func (c *ClientImpl) Site() *url.URL {
	return c.base
}

func (c *ClientImpl) Tokens() *Tokens {
	c.tokenMutex.Lock()
	defer c.tokenMutex.Unlock()
	if c.tokenState != nil && c.tokenState.s != nil {
		return &c.tokenState.s.tokens
	}
	return nil
}

func (c *ClientImpl) User() *User {
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
	defer c.tokenMutex.Unlock()
	c.tokenState = newTokenState

	return nil
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
		defer c.tokenMutex.Unlock()

		c.tokenState = newTokenState
	}

	return c.client.do(method, path, headers, body, queryParams)
}

func (c *ClientImpl) updateTokens(tokens *Tokens) (*Tokens, error) {
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

func (c *ClientImpl) getHeadersAndRefreshToken() *HeadersAndRefreshToken {
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

func (c *ClientImpl) getHeadersAndRefreshTokenIfExpired() ([]Header, *string) {
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

func doRefreshToken(client *thinClient, headers []Header, refreshToken string) (*TokenState, error) {
	type RefreshRequest struct {
		RefreshToken string `json:"refresh_token"`
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

var jsonHeader Header = Header{key: "Content-Type", value: "application/json"}

const authApi string = "api/auth/v1"
