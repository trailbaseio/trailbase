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
	headers map[string]string
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
			headers: make(map[string]string),
		}, nil
	}

	return &TokenState{
		s:       nil,
		headers: make(map[string]string),
	}, nil
}

type JwtTokenClaims struct {
	Email     string `json:"email"`
	CsrfToken string `json:"csrf_token"`
	jwt.RegisteredClaims
}

type Credentials struct {
	Email    string `json:"email"`
	Password string `json:"password"`
}

type Client interface {
	Login(email string, password string) (*Tokens, error)
}

type ClientImpl struct {
	base   *url.URL
	client *http.Client

	tokenState *TokenState
	tokenMutex *sync.Mutex
}

func (c *ClientImpl) Login(email string, password string) (*Tokens, error) {
	creds, err := json.Marshal(Credentials{
		Email:    email,
		Password: password,
	})
	if err != nil {
		return nil, err
	}

	url := c.base.JoinPath(authApi, "login").String()
	resp, err := c.client.Post(url, "application/json", bytes.NewBuffer(creds))
	if err != nil {
		return nil, err
	}

	fmt.Println(resp)

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}

	var tokens Tokens
	err = json.Unmarshal(body, &tokens)
	if err != nil {
		return nil, err
	}

	return c.updateTokens(&tokens)
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

	fmt.Println(tokens)
}

const authApi string = "api/auth/v1"
const recordApi string = "api/records/v1"
