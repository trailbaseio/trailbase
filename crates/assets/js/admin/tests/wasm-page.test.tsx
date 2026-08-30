import { cleanup, render, screen } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  name: "trailbase/auth_ui" as string | undefined,
  loading: true,
  error: false,
  data: undefined as
    | {
        components: Array<{
          name: string;
          path: string;
          loaded: boolean;
          installed: boolean;
        }>;
      }
    | undefined,
  refetch: vi.fn(),
}));

vi.mock("@solidjs/router", () => ({
  useParams: () => state,
}));
vi.mock("@tanstack/solid-query", () => ({
  useQuery: () => ({
    get data() {
      return state.data;
    },
    get isLoading() {
      return state.loading;
    },
    get isError() {
      return state.error;
    },
    refetch: state.refetch,
  }),
}));
vi.mock("@/lib/api/wasm-components", () => ({ listWasmComponents: vi.fn() }));
vi.mock("@/components/wasm/WasmComponentDetails", () => ({
  WasmComponentDetails: () => <div>DETAILS</div>,
}));
vi.mock("@/components/wasm/WasmComponentsList", () => ({
  WasmComponentsList: (props: { isLoading: boolean; isError: boolean }) => (
    <div>
      {props.isLoading ? "LIST LOADING" : props.isError ? "LIST ERROR" : "LIST"}
    </div>
  ),
}));

import { WasmPage } from "@/components/wasm/WasmPage";

afterEach(() => {
  cleanup();
  state.name = "trailbase/auth_ui";
  state.loading = true;
  state.error = false;
  state.data = undefined;
});

describe("WasmPage detail routing", () => {
  it("keeps a detail route in loading state until components resolve", () => {
    render(() => <WasmPage />);
    expect(screen.getByText("LIST LOADING")).toBeInTheDocument();
    expect(screen.queryByText(/not installed/i)).not.toBeInTheDocument();
  });
});
