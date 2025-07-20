package main

import (
	"bytes"
	"fmt"
	"io"
	"sync"

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

type TokenState struct {
	s       *state
	headers http.Header
}

func NewTokenState(tokens *Tokens) (*TokenState, error) {
	if tokens != nil {
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
			headers: make(http.Header),
		}, nil
	}

	return &TokenState{
		s:       nil,
		headers: make(http.Header),
	}, nil
}

type JwtTokenClaims struct {
	Email     string `json:"email"`
	CsrfToken string `json:"csrf_token"`
	jwt.RegisteredClaims
}

type Client interface {
	Login(email string, password string) (*Tokens, error)
	Logout() error
}

type ClientImpl struct {
	base   *url.URL
	client *http.Client

	tokenState *TokenState
	tokenMutex *sync.Mutex
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

	url := c.base.JoinPath(authApi, "login").String()
	req, err := http.NewRequest("POST", url, bytes.NewBuffer(reqBody))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := c.client.Do(req)
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
	r := c.getHeaderAndRefresh()
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

		req, err := http.NewRequest("POST", url, bytes.NewBuffer(body))
		if err != nil {
			return err
		}
		req.Header.Set("Content-Type", "application/json")
		_, err = c.client.Do(req)
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

type HeaderAndRefreshToken struct {
	header       http.Header
	refreshToken string
}

func (c *ClientImpl) getHeaderAndRefresh() *HeaderAndRefreshToken {
	var r *HeaderAndRefreshToken

	c.tokenMutex.Lock()
	s := c.tokenState
	if s != nil && s.s != nil && s.s.tokens.RefreshToken != nil {
		r = &HeaderAndRefreshToken{
			header:       c.tokenState.headers,
			refreshToken: *c.tokenState.s.tokens.RefreshToken,
		}
	}
	c.tokenMutex.Unlock()

	return r
}

func NewClient(site string) (Client, error) {
	base, err := url.Parse(site)
	if err != nil {
		return nil, err
	}
	return &ClientImpl{
		base:       base,
		client:     &http.Client{},
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

	err = client.Logout()
	if err != nil {
		panic(err)
	}
}

const authApi string = "api/auth/v1"
const recordApi string = "api/records/v1"
