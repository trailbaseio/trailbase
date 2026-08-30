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
  A: (props: { href: string; children: import("solid-js").JSX.Element }) => (
    <a href={props.href}>{props.children}</a>
  ),
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
        {props.isLoading
          ? "LIST LOADING"
          : props.isError
            ? "LIST ERROR"
            : "LIST"}
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
    state.data = {
      components: [
        {
          name: "trailbase/auth_ui",
          path: "components/auth_ui.wasm",
          loaded: true,
          installed: true,
        },
      ],
    };
    render(() => <WasmPage />);
    expect(screen.getByText("LIST LOADING")).toBeInTheDocument();
    expect(screen.queryByText("DETAILS")).not.toBeInTheDocument();
    expect(screen.queryByText(/not installed/i)).not.toBeInTheDocument();
  });

  it("keeps a detail route in error state when a refresh fails", () => {
    state.loading = false;
    state.error = true;
    state.data = {
      components: [
        {
          name: "trailbase/auth_ui",
          path: "components/auth_ui.wasm",
          loaded: true,
          installed: true,
        },
      ],
    };
    render(() => <WasmPage />);

    expect(screen.getByText("LIST ERROR")).toBeInTheDocument();
    expect(screen.queryByText("DETAILS")).not.toBeInTheDocument();
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

  it("shows a component-specific unknown state with a return action", () => {
    state.name = "missing";
    state.loading = false;
    state.data = {
      components: [
        {
          name: "installed",
          path: "components/installed.wasm",
          loaded: true,
          installed: true,
        },
      ],
    };
    render(() => <WasmPage />);

    expect(screen.getByText(/not installed/i)).toBeInTheDocument();
    expect(
      screen.getByRole("link", { name: /back to wasm components/i }),
    ).toHaveAttribute("href", "/wasm");
  });
});
