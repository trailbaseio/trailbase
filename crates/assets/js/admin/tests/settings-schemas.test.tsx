import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { JsonSchema } from "@bindings/JsonSchema";

const state = vi.hoisted(() => ({
  schemas: [] as JsonSchema[],
  loading: false,
  error: false,
  postgres: false,
  infoLoading: false,
  infoError: false,
  setSchemas: undefined as ((value: JsonSchema[]) => void) | undefined,
  setLoading: undefined as ((value: boolean) => void) | undefined,
  setError: undefined as ((value: boolean) => void) | undefined,
  setPostgres: undefined as ((value: boolean) => void) | undefined,
  setInfoLoading: undefined as ((value: boolean) => void) | undefined,
  setInfoError: undefined as ((value: boolean) => void) | undefined,
}));
vi.mock("@tanstack/solid-query", async () => {
  const { createSignal } =
    await vi.importActual<typeof import("solid-js")>("solid-js");
  const [schemas, setSchemas] = createSignal<JsonSchema[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal(false);
  state.setSchemas = setSchemas;
  state.setLoading = setLoading;
  state.setError = setError;
  return {
    useQuery: () => ({
      get data() {
        return { schemas: schemas() };
      },
      get isLoading() {
        return loading();
      },
      get isError() {
        return error();
      },
      get isSuccess() {
        return !loading() && !error();
      },
    }),
  };
});
vi.mock("@/lib/api/info", async () => {
  const { createSignal } =
    await vi.importActual<typeof import("solid-js")>("solid-js");
  const [postgres, setPostgres] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal(false);
  state.setPostgres = setPostgres;
  state.setInfoLoading = setLoading;
  state.setInfoError = setError;
  return {
    createSystemInfoQuery: () => ({
      get data() {
        return { postgres: postgres() };
      },
      get isLoading() {
        return loading();
      },
      get isError() {
        return error();
      },
    }),
  };
});
vi.mock("@/lib/fetch", () => ({ adminFetch: vi.fn() }));
import {
  formatSchemaSource,
  SchemaSettings,
} from "@/components/settings/SchemaSettings";

const schemas: JsonSchema[] = [
  { name: "zeta", schema: '{"z":1}', builtin: false },
  { name: "Alpha", schema: '{"a":true}', builtin: true },
  { name: "unsafe", schema: "<script>alert(1)</script>", builtin: false },
];
const setup = () =>
  render(() => <SchemaSettings setDirty={vi.fn()} postSubmit={vi.fn()} />);
beforeEach(() => {
  state.schemas = schemas.map((s) => ({ ...s }));
  state.loading = false;
  state.error = false;
  state.postgres = false;
  state.infoLoading = false;
  state.infoError = false;
  state.setSchemas?.(state.schemas);
  state.setLoading?.(state.loading);
  state.setError?.(state.error);
  state.setPostgres?.(state.postgres);
  state.setInfoLoading?.(state.infoLoading);
  state.setInfoError?.(state.infoError);
});
afterEach(cleanup);

describe("SchemaSettings", () => {
  it("shows schema loading status", () => {
    state.setLoading?.(true);
    setup();
    expect(screen.getByRole("status")).toHaveTextContent(/loading schemas/i);
  });
  it("shows system loading independently", () => {
    state.setInfoLoading?.(true);
    setup();
    expect(screen.getByRole("status")).toHaveTextContent(/loading schemas/i);
  });
  it("shows generic schema error", () => {
    state.setError?.(true);
    setup();
    expect(screen.getByRole("alert")).toHaveTextContent(
      /unable to load schemas/i,
    );
  });
  it("shows generic system error", () => {
    state.setInfoError?.(true);
    setup();
    expect(screen.getByRole("alert")).toHaveTextContent(
      /unable to load system/i,
    );
  });
  it("shows explicit empty state", () => {
    state.setSchemas?.([]);
    setup();
    expect(screen.getByText("No schemas available.")).toBeInTheDocument();
  });
  it("shows only built-ins and hides custom registration in Postgres", () => {
    state.setPostgres?.(true);
    setup();
    expect(screen.getByText(/not supported in postgres/i)).toBeInTheDocument();
    expect(screen.getByText("built-in")).toBeInTheDocument();
    expect(screen.getByText("Alpha")).toBeInTheDocument();
    expect(screen.queryByText("zeta")).toBeNull();
    expect(screen.queryByText("unsafe")).toBeNull();
    expect(screen.queryByText(/registration via the admin ui/i)).toBeNull();
    expect(screen.queryByText(/CREATE TABLE/)).toBeNull();
  });
  it("sorts a copy without mutating query data", () => {
    setup();
    expect(screen.getAllByRole("button").map((x) => x.textContent)).toEqual([
      "Alphabuilt-in",
      "unsafe",
      "zeta",
    ]);
    expect(state.schemas[0].name).toBe("zeta");
  });
  it("filters case-insensitively and clears search", () => {
    setup();
    const input = screen.getByRole("searchbox");
    fireEvent.input(input, { target: { value: "ALP" } });
    expect(screen.getByText("Alpha")).toBeInTheDocument();
    expect(screen.queryByText("zeta")).toBeNull();
    fireEvent.input(input, { target: { value: "" } });
    expect(screen.getByText("zeta")).toBeInTheDocument();
  });
  it("shows a distinct no-results state", () => {
    setup();
    fireEvent.input(screen.getByRole("searchbox"), {
      target: { value: "missing" },
    });
    expect(
      screen.getByText("No schemas match your search."),
    ).toBeInTheDocument();
  });
  it("shows names and only built-in badges", () => {
    setup();
    expect(screen.getByText("Alpha")).toBeInTheDocument();
    expect(screen.getByText("zeta")).toBeInTheDocument();
    expect(screen.getAllByText("built-in")).toHaveLength(1);
  });
  it("opens accordion and pretty prints valid JSON", () => {
    setup();
    fireEvent.click(screen.getByRole("button", { name: /Alpha/ }));
    expect(screen.getByText(/"a": true/)).toBeInTheDocument();
  });
  it("renders malformed JSON as source text", () => {
    setup();
    fireEvent.click(screen.getByRole("button", { name: /unsafe/ }));
    expect(screen.getByText("<script>alert(1)</script>")).toBeInTheDocument();
  });
  it("renders persistent explanation and example SQL", () => {
    setup();
    expect(
      screen.getByText(/registration via the admin ui/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/CREATE TABLE/)).toBeInTheDocument();
  });
  it("has no form or save controls and never submits", () => {
    const post = vi.fn();
    const setDirty = vi.fn();
    render(() => <SchemaSettings setDirty={setDirty} postSubmit={post} />);
    expect(document.querySelector("form")).toBeNull();
    expect(screen.queryByText(/save changes/i)).toBeNull();
    expect(setDirty).toHaveBeenLastCalledWith(false);
    expect(post).not.toHaveBeenCalled();
  });
  it("formats valid and preserves malformed source", () => {
    expect(formatSchemaSource('{"a":1}')).toContain('"a": 1');
    expect(formatSchemaSource("not json")).toBe("not json");
  });
  it("bounds oversized, deeply nested, and hostile sources safely", () => {
    const oversized = `{"value":"${"x".repeat(60_000)}"}`;
    expect(formatSchemaSource(oversized)).toContain("… truncated");
    expect(
      formatSchemaSource("{".repeat(101) + "0" + "}".repeat(101)),
    ).not.toContain('"0"');
    expect(formatSchemaSource('{"value":"escaped \\"{[]}\\""}')).toContain(
      '"value": "escaped',
    );
    expect(formatSchemaSource(null)).toBe("");
    expect(formatSchemaSource({ bad: true })).toContain("[object Object]");
  });
  it("caps oversized malformed sources and keeps source text", () => {
    expect(formatSchemaSource("<script>" + "x".repeat(60_000))).toContain(
      "… truncated",
    );
  });
  it("reacts to loading and error transitions", async () => {
    setup();
    state.setLoading?.(true);
    expect(screen.getByRole("status")).toHaveTextContent(/loading schemas/i);
    state.setLoading?.(false);
    state.setError?.(true);
    await vi.waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        /unable to load schemas/i,
      ),
    );
  });
  it("reacts to Postgres and schema refresh transitions", async () => {
    setup();
    state.setPostgres?.(true);
    expect(screen.queryByText("zeta")).toBeNull();
    state.setPostgres?.(false);
    state.setSchemas?.([
      ...schemas,
      { name: "new-schema", schema: "{}", builtin: false },
    ]);
    await vi.waitFor(() => {
      expect(screen.getByText("new-schema")).toBeInTheDocument();
      expect(screen.getAllByRole("button")[0]).toHaveTextContent("Alpha");
    });
    expect(state.schemas[0].name).toBe("zeta");
  });
  it("announces result counts and no-results search state", () => {
    setup();
    expect(screen.getByRole("status")).toHaveTextContent("3 schemas shown");
    fireEvent.input(screen.getByRole("searchbox"), {
      target: { value: "missing" },
    });
    expect(screen.getByRole("status")).toHaveTextContent(
      "No schemas match your search.",
    );
  });
  it("does not expose hostile source as markup", () => {
    setup();
    fireEvent.click(screen.getByRole("button", { name: /unsafe/ }));
    expect(document.querySelector("script")).toBeNull();
  });
});
