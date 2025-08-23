package trailbase

import (
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"os/exec"
	"path"
	"runtime"
	"strings"
	"time"

	"testing"
)

const (
	PORT uint16 = 4059
	SITE string = "http://127.0.0.1:4059"
)

func buildCommand(name string, cwd string, arg ...string) *exec.Cmd {
	c := exec.Command(name, arg...)
	c.Dir = cwd
	c.Stdout = os.Stdout
	// TODO: Print stdout only if command fails.
	// c.Stderr = os.Stderr
	return c
}

func startTrailBase() (*exec.Cmd, error) {
	cwd := "../../../"
	traildepot := "client/testfixture"

	_, err := os.Stat(path.Join(cwd, traildepot))
	if err != nil {
		return nil, errors.New(fmt.Sprint("missing traildepot: ", err))
	}

	// First build separately to avoid health timeouts.
	err = buildCommand("cargo", cwd, "build").Run()
	if err != nil {
		return nil, err
	}

	// Then start
	args := []string{
		"run",
		"--",
		fmt.Sprint("--data-dir=", traildepot),
		"run",
		fmt.Sprintf("--address=127.0.0.1:%d", PORT),
		"--js-runtime-threads=2",
	}
	cmd := buildCommand("cargo", cwd, args...)
	cmd.Start()

	for i := range 100 {
		if (i+1)%10 == 0 {
			log.Printf("Checking healthy: (%d/100)\n", i+1)
		}

		resp, err := http.Get(fmt.Sprintf("%s/api/healthcheck", SITE))
		if err == nil {
			body, err := io.ReadAll(resp.Body)
			if err != nil {
				return cmd, err
			}

			// Got healthy.
			if strings.ToUpper(string(body)) == "OK" {
				log.Printf("TrailBase became healthy after (%d/100)", i)
				return cmd, nil
			}
		}

		time.Sleep(500 * time.Millisecond)
	}

	return cmd, errors.New("TB server never got healthy")
}

func stopTrailBase(cmd *exec.Cmd) {
	if cmd != nil {
		log.Println("Stopping TrailBase.")

		err := cmd.Process.Kill()
		if err != nil {
			log.Fatal("Failed to kill TB: ", err)
		}
	}
}

func connect(t *testing.T) Client {
	client, err := NewClient(SITE)
	if err != nil {
		panic(err)
	}
	tokens, err := client.Login("admin@localhost", "secret")
	if err != nil {
		t.Fatal(err)
	}
	if tokens == nil {
		t.Fatal("Missing tokens")
	}
	return client
}

// / Separate main function to make defer work, otherwise os.Exit will terminate right away.
func run(m *testing.M) int {
	log.Println("Starting TrailBase.")
	cmd, err := startTrailBase()
	defer stopTrailBase(cmd)

	if err != nil {
		log.Fatal("Failed to start TB: ", err)
	}

	return m.Run()
}

func TestMain(m *testing.M) {
	os.Exit(run(m))
}

func TestAuth(t *testing.T) {
	client := connect(t)

	user := client.User()
	assertEqual(t, user.Email, "admin@localhost")
	assert(t, client.Tokens().RefreshToken != nil, "missing token")

	newClient, err := NewClientWithTokens(SITE, client.Tokens())
	assertFine(t, err)
	assertEqual(t, newClient.User().Email, "admin@localhost")

	client.Refresh()

	err = client.Logout()
	assertFine(t, err)
	assert(t, client.Tokens() == nil, "should be nil")
	assert(t, client.User() == nil, "should be nil")
}

type SimpleStrict struct {
	Id *string `json:"id,omitempty"`

	TextNull    *string `json:"text_null,omitempty"`
	TextDefault *string `json:"text_default,omitempty"`
	TextNotNull string  `json:"text_not_null"`
}

func TestRecordApi(t *testing.T) {
	client := connect(t)
	api := NewRecordApi[SimpleStrict](client, "simple_strict_table")

	now := time.Now().Unix()
	messages := []string{
		fmt.Sprint("go client test 0: =?&", now),
		fmt.Sprint("go client test 1: =?&", now),
	}

	ids := []RecordId{}
	for _, message := range messages {
		id, err := api.Create(SimpleStrict{
			TextNotNull: message,
		})
		assertFine(t, err)
		ids = append(ids, id)
	}

	// Read
	simpleStrict0, err := api.Read(ids[0])
	assertFine(t, err)
	assertEqual(t, messages[0], simpleStrict0.TextNotNull)

	// List specific message
	{
		filters := []Filter{
			FilterColumn{
				Column: "text_not_null",
				Value:  messages[0],
			},
		}
		first, err := api.List(&ListArguments{
			Filters: filters,
		})
		assertFine(t, err)
		assert(t, len(first.Records) == 1, fmt.Sprint("expected 1, got ", first))

		second, err := api.List(&ListArguments{
			Filters: filters,
			Pagination: Pagination{
				Cursor: first.Cursor,
			},
		})
		assertFine(t, err)
		assert(t, len(second.Records) == 0, fmt.Sprint("expected 0, got ", second))
	}

	// List all messages
	{
		filters := []Filter{
			FilterColumn{
				Column: "text_not_null",
				Op:     Like,
				Value:  fmt.Sprint("% =?&", now),
			},
		}

		ascending, err := api.List(&ListArguments{
			Order:   []string{"+text_not_null"},
			Filters: filters,
			Count:   true,
		})
		assertFine(t, err)
		assertEqual(t, 2, *ascending.TotalCount)
		for i, msg := range ascending.Records {
			assertEqual(t, messages[i], msg.TextNotNull)
		}

		descending, err := api.List(&ListArguments{
			Order:   []string{"-text_not_null"},
			Filters: filters,
			Count:   true,
		})
		assertFine(t, err)
		assertEqual(t, 2, *descending.TotalCount)
		for i, msg := range descending.Records {
			assertEqual(t, messages[len(messages)-i-1], msg.TextNotNull)
		}
	}

	// Update
	updatedMessage := fmt.Sprint("go client updated test 0: =?&", now)
	err = api.Update(ids[0], SimpleStrict{
		TextNotNull: updatedMessage,
	})
	assertFine(t, err)
	simpleStrict1, err := api.Read(ids[0])
	assertFine(t, err)
	assertEqual(t, updatedMessage, simpleStrict1.TextNotNull)

	// Delete
	err = api.Delete(ids[0])
	assertFine(t, err)
	r, err := api.Read(ids[0])
	assert(t, err != nil, "expected error reading delete record")
	assert(t, r == nil, "expected nil value reading delete record")
}

func TestTransaction(t *testing.T) {
	client := connect(t)
	batch := client.Transaction()
	api := NewRecordApi[SimpleStrict](client, "simple_strict_table")

	now := time.Now().Unix()

	var ids []RecordId

	// Create
	createdMessage := fmt.Sprint("go transaction create test: =?&", now)
	batch.API("simple_strict_table").Create(SimpleStrict{
		TextNotNull: createdMessage,
	})
	ids, err := batch.Send()
	assertFine(t, err)
	assert(t, len(ids) == 1, "Expected one ID from create operation")

	simpleStrict1, err := api.Read(ids[0])
	assertFine(t, err)
	assertEqual(t, createdMessage, simpleStrict1.TextNotNull)

	// Update
	{
		updatedMessage := fmt.Sprint("go transaction update test: =?&", now)
		batch.API("simple_strict_table").Update(ids[0], SimpleStrict{
			TextNotNull: updatedMessage,
		})
		_, err := batch.Send()
		assertFine(t, err)

		simpleStrict2, err := api.Read(ids[0])
		assertFine(t, err)
		assertEqual(t, updatedMessage, simpleStrict2.TextNotNull)
	}
	// Delete
	{
		batch.API("simple_strict_table").Delete(ids[0])
		_, err := batch.Send()
		assertFine(t, err)
		r, err := api.Read(ids[0])
		assert(t, err != nil, "expected error reading delete record")
		assert(t, r == nil, "expected nil value reading delete record")
	}
}

func assertEqual[T comparable](t *testing.T, expected T, got T) {
	if expected != got {
		buf := make([]byte, 1<<16)
		runtime.Stack(buf, true)
		t.Fatal("Expected", expected, ", got:", got, "\n", string(buf))
	}
}

func assertFine(t *testing.T, err error) {
	if err != nil {
		buf := make([]byte, 1<<16)
		runtime.Stack(buf, true)
		t.Fatal(err, "\n", string(buf))
	}
}

func assert(t *testing.T, condition bool, msg string) {
	if !condition {
		buf := make([]byte, 1<<16)
		runtime.Stack(buf, true)
		t.Fatal(msg, "\n", string(buf))
	}
}
