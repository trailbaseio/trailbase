import { cleanup, render, screen } from "@solidjs/testing-library";
import { onMount } from "solid-js";
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
  refetchCallback: undefined as (() => Promise<void>) | undefined,
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
  WasmComponentsList: (props: {
    isLoading: boolean;
    isError: boolean;
    refetch: () => Promise<void>;
  }) => {
    onMount(() => {
      state.refetchCallback = props.refetch;
    });
    return (
      <div>
        {props.isLoading ? "LIST LOADING" : props.isError ? "LIST ERROR" : "LIST"}
      </div>
    );
  },
}));

import { WasmPage } from "@/components/wasm/WasmPage";

afterEach(() => {
  cleanup();
  state.name = "trailbase/auth_ui";
  state.loading = true;
  state.error = false;
  state.data = undefined;
  state.refetchCallback = undefined;
});

describe("WasmPage detail routing", () => {
  it("keeps a detail route in loading state until components resolve", () => {
    render(() => <WasmPage />);
    expect(screen.getByText("LIST LOADING")).toBeInTheDocument();
    expect(screen.queryByText(/not installed/i)).not.toBeInTheDocument();
  });

  it("normalizes refresh failures into a rejecting Promise<void>", async () => {
    state.name = undefined;
    state.loading = false;
    const failure = new Error("refresh failed");
    state.refetch.mockImplementation((options?: { throwOnError?: boolean }) =>
      options?.throwOnError
        ? Promise.reject(failure)
        : Promise.resolve({ isError: true }),
    );
    render(() => <WasmPage />);

    await expect(state.refetchCallback?.()).rejects.toBe(failure);
    expect(state.refetch).toHaveBeenCalledWith({ throwOnError: true });
  });
});
