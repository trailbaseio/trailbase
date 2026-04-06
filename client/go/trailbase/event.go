package trailbase

import (
	"bytes"

	"encoding/json"
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

type ErrorEvent struct {
	Status  int64
	Message *string
}

type Event struct {
	Seq   *int64
	Value ValueEvent
	Error *ErrorEvent
}

func parseEvent(msg []byte) (*Event, error) {
	if !bytes.HasPrefix(msg, []byte("data: ")) {
		return nil, nil
	}

	var evMap map[string]any
	err := json.Unmarshal(msg[6:], &evMap)
	if err != nil {
		return nil, err
	}

	var seq *int64 = nil
	seqf, ok := evMap["seq"].(float64)
	if ok {
		seqi := int64(seqf)
		seq = &seqi
	}

	if val, ok := evMap["Error"]; ok {
		var errObj = val.(map[string]any)
		var msg, ok = errObj["message"].(string)
		if ok {
			return &Event{
				Seq: seq,
				Error: &ErrorEvent{
					Status:  int64(errObj["status"].(float64)),
					Message: &msg,
				},
			}, nil
		}

		return &Event{
			Seq: seq,
			Error: &ErrorEvent{
				Status: int64(errObj["status"].(float64)),
			},
		}, nil
	} else if val, ok := evMap["Insert"]; ok {
		return &Event{
			Seq: seq,
			Value: &InsertEvent{
				value: val.(map[string]any),
			},
		}, nil
	} else if val, ok := evMap["Update"]; ok {
		return &Event{
			Seq: seq,
			Value: &UpdateEvent{
				value: val.(map[string]any),
			},
		}, nil
	} else if val, ok := evMap["Delete"]; ok {
		return &Event{
			Seq: seq,
			Value: &DeleteEvent{
				value: val.(map[string]any),
			},
		}, nil
	}

	return nil, nil
}
