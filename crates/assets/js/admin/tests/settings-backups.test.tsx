import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  backups: { backups: [] as { timestamp: bigint }[] },
  loading: false,
  error: false,
  refetch: vi.fn(),
  trigger: vi.fn(),
  restore: vi.fn(),
  del: vi.fn(),
  toast: vi.fn(),
}));
vi.mock("@tanstack/solid-query", async () => {
  const { createSignal } =
    await vi.importActual<typeof import("solid-js")>("solid-js");
  return {
    useQuery: () => ({
      get data() {
        return state.backups;
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
      refetch: state.refetch,
    }),
  };
});
vi.mock("@/lib/api/backups", () => ({
  listBackups: vi.fn(),
  triggerBackup: () => state.trigger(),
  restoreBackup: (t: bigint) => state.restore(t),
  deleteBackups: (t: bigint[]) => state.del(t),
}));
vi.mock("@/lib/api/config", () => ({
  createConfigQuery: () => ({ data: undefined }),
}));
vi.mock("@/components/ui/toast", () => ({
  showToast: (x: unknown) => state.toast(x),
}));

import { BackupSettings } from "@/components/settings/BackupSettings";

const timestamp = 1_700_000_000_000n;
const renderBackups = () =>
  render(() => <BackupSettings setDirty={vi.fn()} postSubmit={vi.fn()} />);

beforeEach(() => {
  state.backups = { backups: [] };
  state.loading = false;
  state.error = false;
  state.refetch.mockReset().mockResolvedValue(undefined);
  state.trigger.mockReset().mockResolvedValue(undefined);
  state.restore.mockReset().mockResolvedValue(undefined);
  state.del.mockReset().mockResolvedValue(undefined);
  state.toast.mockReset();
});
afterEach(cleanup);

describe("BackupSettings", () => {
  it("shows loading, generic error, empty, and populated table states", async () => {
    state.loading = true;
    renderBackups();
    expect(screen.getByText("Loading...")).toBeInTheDocument();
    cleanup();
    state.loading = false;
    state.error = true;
    renderBackups();
    expect(screen.getByText(/unable to load backups/i)).toBeInTheDocument();
    expect(screen.queryByText(/raw/i)).not.toBeInTheDocument();
    cleanup();
    state.error = false;
    renderBackups();
    expect(screen.getByText("No backups available.")).toBeInTheDocument();
    cleanup();
    state.backups = { backups: [{ timestamp }] };
    renderBackups();
    expect(screen.getByRole("table")).toBeInTheDocument();
    expect(
      screen.getByText(new Date(Number(timestamp)).toLocaleString()),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /delete backup from/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /restore backup from/i }),
    ).toBeInTheDocument();
  });

  it("awaits trigger and refetches only after success", async () => {
    let resolve!: () => void;
    state.trigger.mockReturnValue(new Promise<void>((r) => (resolve = r)));
    renderBackups();
    const button = screen.getByRole("button", { name: /trigger backup/i });
    fireEvent.click(button);
    expect(button).toBeDisabled();
    expect(state.refetch).not.toHaveBeenCalled();
    resolve();
    await waitFor(() => expect(state.refetch).toHaveBeenCalledOnce());
    expect(state.toast).toHaveBeenCalled();
  });

  it("confirms delete and keeps confirmation open on failure", async () => {
    state.backups = { backups: [{ timestamp }] };
    state.del.mockRejectedValue(new Error("secret backend detail"));
    renderBackups();
    fireEvent.click(
      screen.getByRole("button", { name: /delete backup from/i }),
    );
    expect(screen.getByRole("dialog")).toHaveTextContent(/delete backup/i);
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(/try again/i),
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(
      screen.queryByText(/secret backend detail/i),
    ).not.toBeInTheDocument();
    expect(state.refetch).not.toHaveBeenCalled();
  });
});
