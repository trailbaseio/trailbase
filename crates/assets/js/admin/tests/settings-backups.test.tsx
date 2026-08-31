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

  it("exposes loading as a status", () => {
    state.loading = true;
    renderBackups();
    expect(screen.getByRole("status")).toHaveTextContent("Loading");
  });

  it("shows the empty state", () => {
    renderBackups();
    expect(screen.getByText("No backups available.")).toBeInTheDocument();
  });

  it("renders semantic headers and rolling-window fallback", () => {
    state.backups = { backups: [{ timestamp }] };
    renderBackups();
    expect(
      screen.getByRole("columnheader", { name: "Time" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("columnheader", { name: "Actions" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/Current window size: 5/)).toBeInTheDocument();
  });

  it("disables competing actions during trigger", async () => {
    let resolve!: () => void;
    state.trigger.mockReturnValue(new Promise<void>((r) => (resolve = r)));
    state.backups = { backups: [{ timestamp }] };
    renderBackups();
    fireEvent.click(screen.getByRole("button", { name: /trigger backup/i }));
    expect(
      screen.getByRole("button", { name: /delete backup from/i }),
    ).toBeDisabled();
    resolve();
    await waitFor(() => expect(state.trigger).toHaveBeenCalledOnce());
  });

  it("reports trigger failure generically and allows retry", async () => {
    state.trigger
      .mockRejectedValueOnce(new Error("secret"))
      .mockResolvedValueOnce(undefined);
    renderBackups();
    fireEvent.click(screen.getByRole("button", { name: /trigger backup/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(/try again/i),
    );
    expect(screen.queryByText("secret")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /trigger backup/i }));
    await waitFor(() => expect(state.trigger).toHaveBeenCalledTimes(2));
  });

  it("cancels delete without calling the API", () => {
    state.backups = { backups: [{ timestamp }] };
    renderBackups();
    fireEvent.click(
      screen.getByRole("button", { name: /delete backup from/i }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(state.del).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
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

  it("deletes successfully after refetch and announces completion", async () => {
    state.backups = { backups: [{ timestamp }] };
    renderBackups();
    fireEvent.click(
      screen.getByRole("button", { name: /delete backup from/i }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(state.toast).toHaveBeenCalled());
    expect(state.del).toHaveBeenCalledWith([timestamp]);
    expect(state.refetch).toHaveBeenCalledOnce();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("opens restore confirmation with a readable timestamp", () => {
    state.backups = { backups: [{ timestamp }] };
    renderBackups();
    fireEvent.click(
      screen.getByRole("button", { name: /restore backup from/i }),
    );
    expect(screen.getByRole("dialog")).toHaveTextContent(/restore backup/i);
    expect(screen.getByRole("dialog")).toHaveTextContent(
      new Date(Number(timestamp)).toLocaleString(),
    );
  });

  it("cancels restore without calling the API", () => {
    state.backups = { backups: [{ timestamp }] };
    renderBackups();
    fireEvent.click(
      screen.getByRole("button", { name: /restore backup from/i }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(state.restore).not.toHaveBeenCalled();
  });

  it("reports restore failure without refresh or toast", async () => {
    state.backups = { backups: [{ timestamp }] };
    state.restore.mockRejectedValue(new Error("secret restore"));
    renderBackups();
    fireEvent.click(
      screen.getByRole("button", { name: /restore backup from/i }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(/try again/i),
    );
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(state.refetch).not.toHaveBeenCalled();
    expect(state.toast).not.toHaveBeenCalled();
  });

  it("restores successfully after refresh and announces completion", async () => {
    state.backups = { backups: [{ timestamp }] };
    renderBackups();
    fireEvent.click(
      screen.getByRole("button", { name: /restore backup from/i }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(state.toast).toHaveBeenCalled());
    expect(state.restore).toHaveBeenCalledWith(timestamp);
    expect(state.refetch).toHaveBeenCalledOnce();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("names actions for every row", () => {
    const second = timestamp + 1000n;
    state.backups = { backups: [{ timestamp }, { timestamp: second }] };
    renderBackups();
    expect(
      screen.getAllByRole("button", { name: /delete backup from/i }),
    ).toHaveLength(2);
    expect(
      screen.getAllByRole("button", { name: /restore backup from/i }),
    ).toHaveLength(2);
  });
});
