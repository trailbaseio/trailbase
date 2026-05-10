export type SimpleStrict = {
  id: string;

  text_null?: string;
  text_default: string;
  text_not_null: string;

  int_null?: bigint;
  int_default: bigint;
  int_not_null: bigint;

  // Add or generate missing fields.
};

export type NewSimpleStrict = Partial<SimpleStrict>;

export type SimpleCompleteView = SimpleStrict;

export type SimpleSubsetView = {
  id: string;

  t_null?: string;
  t_default?: string;
  t_not_null: string;
};
