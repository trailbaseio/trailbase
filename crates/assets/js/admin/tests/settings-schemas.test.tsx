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
}));
vi.mock("@tanstack/solid-query", () => ({
  useQuery: () => ({
    get data() {
      return { schemas: state.schemas };
    },
    get isLoading() {
      return state.loading;
    },
    get isError() {
      return state.error;
    },
    get isSuccess() {
      return !state.loading && !state.error;
    },
  }),
}));
vi.mock("@/lib/api/info", () => ({
  createSystemInfoQuery: () => ({
    get data() {
      return { postgres: state.postgres };
    },
    get isLoading() {
      return state.infoLoading;
    },
    get isError() {
      return state.infoError;
    },
  }),
}));
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
});
afterEach(cleanup);

describe("SchemaSettings", () => {
  it("shows schema loading status", () => {
    state.loading = true;
    setup();
    expect(screen.getByRole("status")).toHaveTextContent(/loading schemas/i);
  });
  it("shows system loading independently", () => {
    state.infoLoading = true;
    setup();
    expect(screen.getByRole("status")).toHaveTextContent(/loading schemas/i);
  });
  it("shows generic schema error", () => {
    state.error = true;
    setup();
    expect(screen.getByRole("alert")).toHaveTextContent(
      /unable to load schemas/i,
    );
  });
  it("shows generic system error", () => {
    state.infoError = true;
    setup();
    expect(screen.getByRole("alert")).toHaveTextContent(
      /unable to load system/i,
    );
  });
  it("shows explicit empty state", () => {
    state.schemas = [];
    setup();
    expect(screen.getByText("No schemas available.")).toBeInTheDocument();
  });
  it("shows Postgres explanation and built-ins", () => {
    state.postgres = true;
    setup();
    expect(screen.getByText(/not supported in postgres/i)).toBeInTheDocument();
    expect(screen.getByText("built-in")).toBeInTheDocument();
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
  it("does not expose hostile source as markup", () => {
    setup();
    fireEvent.click(screen.getByRole("button", { name: /unsafe/ }));
    expect(document.querySelector("script")).toBeNull();
  });
});
