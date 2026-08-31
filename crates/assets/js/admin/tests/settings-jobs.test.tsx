import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Config, JobsConfig, SystemJob } from "@proto/config";
import type { Job } from "@bindings/Job";

const state = vi.hoisted(() => ({
  config: undefined as Config | undefined,
  configLoading: false,
  configError: false,
  jobs: [] as Job[],
  jobsLoading: false,
  jobsError: false,
  setConfig: vi.fn(),
  runJob: vi.fn(),
  invalidateQueries: vi.fn(),
  showToast: vi.fn(),
  setConfigSignal: undefined as ((v: Config | undefined) => void) | undefined,
  setJobsSignal: undefined as ((v: Job[]) => void) | undefined,
}));

vi.mock("@tanstack/solid-query", async () => {
  const Solid = await vi.importActual<typeof import("solid-js")>("solid-js");
  return {
    useQueryClient: () => ({
      invalidateQueries: state.invalidateQueries,
      getQueryData: () => ({ hash: "hash" }),
    }),
    useQuery: () => {
      const [jobs, setJobs] = Solid.createSignal(state.jobs);
      state.setJobsSignal = setJobs;
      return {
        get data() {
          return { jobs: jobs() };
        },
        get isLoading() {
          return state.jobsLoading;
        },
        get isError() {
          return state.jobsError;
        },
        get isSuccess() {
          return !state.jobsLoading && !state.jobsError;
        },
        error: new Error("raw jobs detail"),
      };
    },
  };
});
vi.mock("@/lib/api/config", async () => {
  const Solid = await vi.importActual<typeof import("solid-js")>("solid-js");
  return {
    createConfigQuery: () => {
      const [config, setConfig] = Solid.createSignal(state.config);
      state.setConfigSignal = setConfig;
      return {
        get data() {
          return config() === undefined ? undefined : { config: config() };
        },
        get isLoading() {
          return state.configLoading;
        },
        get isError() {
          return state.configError;
        },
        get error() {
          return state.configError ? new Error("raw config detail") : null;
        },
      };
    },
    setConfig: state.setConfig,
  };
});
vi.mock("@/lib/api/jobs", () => ({
  listJobs: vi.fn(),
  runJob: (request: { id: number }) => state.runJob(request),
}));
vi.mock("@/components/ui/toast", () => ({
  showToast: (value: unknown) => state.showToast(value),
}));

import {
  buildFormProxy,
  equal,
  extractConfig,
  JobSettings,
} from "@/components/settings/JobSettings";

const job = (
  id = 1,
  name = "Cleanup",
  schedule = "@daily",
  enabled = true,
): Job => ({
  id,
  name,
  schedule,
  enabled,
  next: 1_700_000_000n,
  latest: [1_700_000_000n, 1_250n, "raw execution detail"],
});
const baseConfig = () =>
  Config.fromPartial({
    server: { applicationName: "Keep this app" },
    jobs: JobsConfig.fromPartial({
      systemJobs: [
        SystemJob.fromPartial({ id: 1, schedule: "@daily", disabled: false }),
      ],
    }),
  });
function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}
function setup(config = baseConfig(), jobs = [job()]) {
  state.config = config;
  state.jobs = jobs;
  state.configLoading = false;
  state.configError = false;
  state.jobsLoading = false;
  state.jobsError = false;
  const setDirty = vi.fn();
  const postSubmit = vi.fn();
  const view = render(() => (
    <JobSettings setDirty={setDirty} postSubmit={postSubmit} />
  ));
  return { ...view, setDirty, postSubmit };
}
function schedule() {
  return screen.getByLabelText("Schedule for Cleanup") as HTMLInputElement;
}

beforeEach(() => {
  cleanup();
  state.config = baseConfig();
  state.jobs = [job()];
  state.configLoading = false;
  state.configError = false;
  state.jobsLoading = false;
  state.jobsError = false;
  state.setConfig.mockReset().mockResolvedValue(undefined);
  state.runJob.mockReset().mockResolvedValue({ error: null });
  state.invalidateQueries.mockReset().mockResolvedValue(undefined);
  state.showToast.mockReset();
});
afterEach(cleanup);

describe("JobSettings loading and presentation", () => {
  it("shows jobs loading status", () => {
    state.jobsLoading = true;
    render(() => <JobSettings setDirty={vi.fn()} postSubmit={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent(/loading/i);
  });
  it("shows config loading status", () => {
    state.configLoading = true;
    render(() => <JobSettings setDirty={vi.fn()} postSubmit={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent(/loading/i);
  });
  it("shows generic jobs errors without backend detail", () => {
    state.jobsError = true;
    render(() => <JobSettings setDirty={vi.fn()} postSubmit={vi.fn()} />);
    expect(screen.getByRole("alert")).toHaveTextContent(/unable to load jobs/i);
    expect(screen.queryByText(/raw jobs detail/i)).toBeNull();
  });
  it("shows generic config errors without backend detail", () => {
    state.configError = true;
    render(() => <JobSettings setDirty={vi.fn()} postSubmit={vi.fn()} />);
    expect(screen.getByRole("alert")).toHaveTextContent(
      /unable to load configuration/i,
    );
    expect(screen.queryByText(/raw config detail/i)).toBeNull();
  });
  it("shows an explicit empty state", () => {
    setup(baseConfig(), []);
    expect(screen.getByText("No jobs configured.")).toBeInTheDocument();
    expect(screen.getByRole("table")).toBeInTheDocument();
  });
  it("renders dense headers, bounded table, labels, times, duration, and no raw error", () => {
    setup();
    for (const heading of [
      "Name",
      "Schedule",
      "Next Run",
      "Last Run",
      "Enabled",
      "Action",
    ])
      expect(screen.getByText(heading)).toBeInTheDocument();
    expect(document.querySelector(".overflow-x-auto")).toBeTruthy();
    expect(screen.getByLabelText("Schedule for Cleanup")).toBeInTheDocument();
    expect(screen.getByLabelText("Enabled for Cleanup")).toBeInTheDocument();
    expect(screen.getByLabelText("Run Cleanup now")).toBeInTheDocument();
    expect(screen.getByText(/1\.25s/)).toBeInTheDocument();
    expect(screen.queryByText(/raw execution detail/i)).toBeNull();
  });
});

describe("JobSettings editing and saving", () => {
  it("validates cron and disables Save for invalid input", async () => {
    setup();
    await fireEvent.input(schedule(), { target: { value: "not cron" } });
    await fireEvent.blur(schedule());
    expect(screen.getByText("Not a valid cron spec")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save changes" })).toBeDisabled();
    await fireEvent.input(schedule(), { target: { value: "0 0 0 * * *" } });
    expect(screen.queryByText("Not a valid cron spec")).toBeNull();
  });
  it("reports dirty edits and clears dirty after editing back", async () => {
    const v = setup();
    await fireEvent.input(schedule(), { target: { value: "@hourly" } });
    expect(v.setDirty).toHaveBeenLastCalledWith(true);
    await fireEvent.input(schedule(), { target: { value: "@daily" } });
    expect(v.setDirty).toHaveBeenLastCalledWith(false);
  });
  it("Reset restores schedule and enabled values", async () => {
    setup();
    await fireEvent.input(schedule(), { target: { value: "@hourly" } });
    await fireEvent.click(screen.getByLabelText("Enabled for Cleanup"));
    await fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(schedule()).toHaveValue("@daily");
    expect(screen.getByLabelText("Enabled for Cleanup")).toHaveAttribute(
      "data-checked",
      "",
    );
  });
  it("retains edits and shows a generic error when save rejects", async () => {
    state.setConfig.mockRejectedValue(new Error("secret save detail"));
    setup();
    await fireEvent.input(schedule(), { target: { value: "@hourly" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(/unable to save/i),
    );
    expect(schedule()).toHaveValue("@hourly");
    expect(screen.queryByText(/secret save detail/i)).toBeNull();
  });
  it("awaits exact save, invalidation, refresh, and postSubmit while preserving unrelated config", async () => {
    const saved = deferred<void>();
    const refreshed = deferred<unknown>();
    const order: string[] = [];
    state.setConfig.mockImplementation(async (opts: { config: Config }) => {
      order.push("setConfig");
      expect(opts.config.server?.applicationName).toBe("Keep this app");
      expect(opts.config.jobs?.systemJobs[0].schedule).toBe("@hourly");
      saved.resolve();
    });
    state.invalidateQueries.mockImplementation(async () => {
      order.push("invalidate");
      return refreshed.promise;
    });
    const v = setup();
    await fireEvent.input(schedule(), { target: { value: "@hourly" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(state.setConfig).toHaveBeenCalled());
    expect(order).toEqual(["setConfig", "invalidate"]);
    refreshed.resolve(undefined);
    await waitFor(() => expect(state.invalidateQueries).toHaveBeenCalled());
    await waitFor(() => expect(v.postSubmit).toHaveBeenCalled());
    expect(order).toEqual(["setConfig", "invalidate"]);
  });
  it("preserves local edits during incoming remote refresh and rebases clean forms", async () => {
    const v = setup();
    await fireEvent.input(schedule(), { target: { value: "@hourly" } });
    state.setConfigSignal?.(
      Config.fromPartial({
        server: { applicationName: "Remote app" },
        jobs: JobsConfig.fromPartial({
          systemJobs: [
            SystemJob.fromPartial({
              id: 1,
              schedule: "@weekly",
              disabled: false,
            }),
            SystemJob.fromPartial({
              id: 2,
              schedule: "@daily",
              disabled: true,
            }),
          ],
        }),
      }),
    );
    state.setJobsSignal?.([job(), job(2, "Remote", "@daily", false)]);
    await waitFor(() => expect(schedule()).toHaveValue("@hourly"));
    expect(v.setDirty).toHaveBeenLastCalledWith(true);
    await fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(state.setConfig).toHaveBeenCalled());
    const saved = state.setConfig.mock.calls.at(-1)?.[0].config as Config;
    expect(saved.jobs?.systemJobs).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: 2, schedule: "@daily", disabled: true }),
      ]),
    );
  });
  it("does not update callbacks after save completes on unmounted form", async () => {
    const pending = deferred<void>();
    state.setConfig.mockReturnValue(pending.promise);
    const v = setup();
    await fireEvent.input(schedule(), { target: { value: "@hourly" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
    cleanup();
    pending.resolve();
    await pending.promise;
    expect(v.postSubmit).not.toHaveBeenCalled();
  });
});

describe("JobSettings Run now", () => {
  it("awaits refresh, prevents competing runs, and announces success", async () => {
    const pending = deferred<{ error: null }>();
    const refresh = deferred<unknown>();
    state.runJob.mockReturnValue(pending.promise);
    state.invalidateQueries.mockReturnValue(refresh.promise);
    setup();
    const button = screen.getByLabelText("Run Cleanup now");
    await fireEvent.click(button);
    expect(button).toBeDisabled();
    await fireEvent.click(button);
    expect(state.runJob).toHaveBeenCalledTimes(1);
    pending.resolve({ error: null });
    await waitFor(() => expect(state.invalidateQueries).toHaveBeenCalled());
    expect(state.showToast).not.toHaveBeenCalled();
    refresh.resolve(undefined);
    await waitFor(() =>
      expect(state.showToast).toHaveBeenCalledWith(
        expect.objectContaining({ title: "Job started" }),
      ),
    );
  });
  it("handles response errors, rejection, and refresh failures generically", async () => {
    state.runJob.mockResolvedValueOnce({ error: "secret response detail" });
    setup();
    await fireEvent.click(screen.getByLabelText("Run Cleanup now"));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(/unable to run/i),
    );
    expect(screen.queryByText(/secret response detail/i)).toBeNull();
    state.runJob.mockRejectedValueOnce(new Error("secret rejection"));
    await fireEvent.click(screen.getByLabelText("Run Cleanup now"));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(/unable to run/i),
    );
    expect(screen.queryByText(/secret rejection/i)).toBeNull();
  });
  it("does not log secrets or update UI after Run now unmount", async () => {
    const pending = deferred<{ error: null }>();
    state.runJob.mockReturnValue(pending.promise);
    const info = vi.spyOn(console, "info").mockImplementation(() => {});
    setup();
    await fireEvent.click(screen.getByLabelText("Run Cleanup now"));
    cleanup();
    pending.resolve({ error: null });
    await pending.promise;
    expect(info).not.toHaveBeenCalled();
    info.mockRestore();
  });
});

describe("JobSettings protobuf helpers", () => {
  it("keeps configured jobs and emits changed defaults", () => {
    const configured = SystemJob.fromPartial({ id: 1, schedule: "@daily" });
    const proxy = buildFormProxy(
      JobsConfig.fromPartial({ systemJobs: [configured] }),
      [job(), job(2, "Index", "@hourly")],
    );
    expect(extractConfig(proxy).systemJobs).toEqual([configured]);
    proxy.jobs[1].config.schedule = "@daily";
    expect(extractConfig(proxy).systemJobs).toHaveLength(2);
  });
  it("compares exact protobuf job leaves", () => {
    const value = SystemJob.fromPartial({
      id: 1,
      schedule: "@daily",
      disabled: false,
    });
    expect(equal(value, SystemJob.fromPartial({ ...value }))).toBe(true);
    expect(
      equal(value, SystemJob.fromPartial({ ...value, schedule: "@hourly" })),
    ).toBe(false);
  });
});
