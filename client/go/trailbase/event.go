package trailbase

import (
	"errors"
	"fmt"
)

type ValueEvent interface {
	Value() *map[string]any
}

type InsertEvent struct {
	value map[string]any
}

func (ev *InsertEvent) Value() *map[string]any {
	return &ev.value
}

type UpdateEvent struct {
	value map[string]any
}

func (ev *UpdateEvent) Value() *map[string]any {
	return &ev.value
}

type DeleteEvent struct {
	value map[string]any
}

func (ev *DeleteEvent) Value() *map[string]any {
	return &ev.value
}

type Event struct {
	Seq   *uint32
	Value ValueEvent
	Error *string
}

func parseEvent(evMap map[string]any) (*Event, error) {
	var t string
	if val, ok := evMap["type"]; ok {
		t = val.(string)
	} else {
		return nil, errors.New(fmt.Sprintln("unknown event type", evMap))
	}

	var seq *uint32 = nil
	if val, ok := evMap["seq"]; ok {
		s, ok := val.(float64)
		if ok {
			v := uint32(s)
			seq = &v
		}
	}

	switch t {
	case "error":
		if val, ok := evMap["error"]; ok {
			var errString string = val.(string)
			return &Event{
				Seq:   seq,
				Error: &errString,
			}, nil
		}
	case "insert":
		if val, ok := evMap["value"]; ok {
			return &Event{
				Seq: seq,
				Value: &InsertEvent{
					value: val.(map[string]any),
				},
			}, nil
		}
	case "update":
		if val, ok := evMap["value"]; ok {
			return &Event{
				Seq: seq,
				Value: &UpdateEvent{
					value: val.(map[string]any),
				},
			}, nil
		}
	case "delete":
		if val, ok := evMap["value"]; ok {
			return &Event{
				Seq: seq,
				Value: &DeleteEvent{
					value: val.(map[string]any),
				},
			}, nil
		}
	}

	return nil, errors.New(fmt.Sprintln("parsing event failed", evMap))
}
