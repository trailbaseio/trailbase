package trailbase

type jsonOp struct {
	Name  string  `json:"api_name"`
	Id    *string `json:"record_id,omitempty"`
	Value any     `json:"value,omitempty"`
}

type Operation interface {
	json() map[string]jsonOp
}

type CreateOperation[T any] struct {
	ApiName string
	Value   T
}

func (o CreateOperation[T]) json() map[string]jsonOp {
	return map[string]jsonOp{
		"Create": jsonOp{
			Name:  o.ApiName,
			Value: &o.Value,
		},
	}
}

type UpdateOperation[T any] struct {
	ApiName string
	Id      RecordId
	Value   T
}

func (o UpdateOperation[T]) json() map[string]jsonOp {
	id := o.Id.ToString()
	return map[string]jsonOp{
		"Update": jsonOp{
			Name:  o.ApiName,
			Id:    &id,
			Value: &o.Value,
		},
	}
}

type DeleteOperation[T any] struct {
	ApiName string
	Id      RecordId
}

func (o DeleteOperation[T]) json() map[string]jsonOp {
	id := o.Id.ToString()
	return map[string]jsonOp{
		"Delete": jsonOp{
			Name: o.ApiName,
			Id:   &id,
		},
	}
}

type OperationResult struct {
	Id  *StringRecordId `json:"Id,omitempty"`
	Err *string         `json:"Error,omitempty"`
}
