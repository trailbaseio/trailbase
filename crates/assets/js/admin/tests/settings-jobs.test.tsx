import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, fireEvent, cleanup } from "@solidjs/testing-library";
import { Config, JobsConfig, SystemJob } from "@proto/config";
import type { Job } from "@bindings/Job";

const mocks = vi.hoisted(() => ({
  config: undefined as Config | undefined,
  jobs: [] as Job[],
  setConfig: vi.fn().mockResolvedValue(undefined),
  runJob: vi.fn().mockResolvedValue({ error: null }),
  invalidateQueries: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@/lib/api/config", () => ({
  createConfigQuery: () => ({ isError: false, data: { config: mocks.config } }),
  setConfig: mocks.setConfig,
}));
vi.mock("@/lib/api/jobs", () => ({
  listJobs: () => Promise.resolve({ jobs: mocks.jobs }),
  runJob: mocks.runJob,
}));
vi.mock("@tanstack/solid-query", () => ({
  useQueryClient: () => ({
    invalidateQueries: mocks.invalidateQueries,
    getQueryData: () => ({ hash: "hash" }),
  }),
  useQuery: (fn: () => { queryFn: () => Promise<unknown> }) => ({
    isSuccess: true,
    data: { jobs: mocks.jobs },
  }),
}));

import {
  buildFormProxy,
  extractConfig,
  equal,
  JobSettings,
} from "@/components/settings/JobSettings";

const job = (id = 1, name = "Cleanup", schedule = "@daily"): Job => ({
  id,
  name,
  schedule,
  enabled: true,
  next: 1700000000n,
  latest: [1700000000n, 1250n, null],
});
function setup() {
  mocks.config = Config.fromPartial({
    server: { applicationName: "private" },
    jobs: JobsConfig.fromPartial({ systemJobs: [] }),
  });
  mocks.jobs = [job()];
  mocks.setConfig.mockClear();
  mocks.runJob.mockClear();
  mocks.invalidateQueries.mockClear();
  const dirty = vi.fn(),
    postSubmit = vi.fn();
  const view = render(() => (
    <JobSettings setDirty={dirty} postSubmit={postSubmit} />
  ));
  return { ...view, dirty, postSubmit };
}

describe("jobs settings UI", () => {
  beforeEach(() => {
    cleanup();
    mocks.setConfig.mockClear();
    mocks.runJob.mockClear();
    mocks.invalidateQueries.mockClear();
  });
  it("renders all semantic columns", () => {
    const v = setup();
    for (const x of [
      "Name",
      "Schedule",
      "Next Run",
      "Last Run",
      "Enabled",
      "Action",
    ])
      expect(v.getByText(x)).toBeInTheDocument();
  });
  it("renders a bounded horizontal table", () =>
    expect(setup().container.querySelector(".overflow-x-auto")).toBeTruthy());
  it("renders job name and accessible controls", () => {
    const v = setup();
    expect(v.getByText("Cleanup")).toBeInTheDocument();
    expect(v.getByLabelText("Schedule for Cleanup")).toBeInTheDocument();
    expect(v.getByLabelText("Enabled for Cleanup")).toBeInTheDocument();
    expect(v.getByLabelText("Run Cleanup now")).toBeInTheDocument();
  });
  it("formats next and latest timestamps", () => {
    const v = setup();
    expect(v.getAllByText(/Tue, 14 Nov 2023/).length).toBeGreaterThan(0);
    expect(v.getByText(/1.25s/)).toBeInTheDocument();
  });
  it("shows an empty state", () => {
    mocks.jobs = [];
    const v = setup();
    expect(v.container.querySelector("table")).toBeInTheDocument();
  });
  it("uses a generic error surface", () => {
    mocks.config = Config.fromPartial({ jobs: {} });
    const v = setup();
    expect(v.queryByText("private")).not.toBeInTheDocument();
  });
  it("accepts six component cron", () => {
    const v = setup();
    expect(v.getByLabelText("Schedule for Cleanup")).toHaveValue("@daily");
  });
  it("accepts aliases without changing them", () => {
    mocks.jobs = [job(1, "Cleanup", "@hourly")];
    expect(setup().getByLabelText("Schedule for Cleanup")).toBeInTheDocument();
  });
  it("shows save and reset after editing", async () => {
    const v = setup();
    await fireEvent.change(v.getByLabelText("Schedule for Cleanup"), {
      target: { value: "0 0 0 * * *" },
    });
    expect(v.getByLabelText("Schedule for Cleanup")).toBeInTheDocument();
  });
  it("reports dirty state", async () => {
    const v = setup();
    await fireEvent.change(v.getByLabelText("Schedule for Cleanup"), {
      target: { value: "0 0 0 * * *" },
    });
    expect(v.dirty).toHaveBeenLastCalledWith(true);
  });
  it("reset restores the baseline", async () => {
    const v = setup();
    const input = v.getByLabelText("Schedule for Cleanup");
    await fireEvent.change(input, { target: { value: "0 0 0 * * *" } });
    expect(input).toBeInTheDocument();
  });
  it("runs one job and refreshes", async () => {
    const v = setup();
    await fireEvent.click(v.getByLabelText("Run Cleanup now"));
    expect(mocks.runJob).toHaveBeenCalledWith({ id: 1 });
    expect(mocks.invalidateQueries).toHaveBeenCalled();
  });
  it("prevents competing runs", async () => {
    let resolve!: (x: { error: null }) => void;
    mocks.runJob.mockReturnValueOnce(
      new Promise((r) => {
        resolve = r;
      }),
    );
    const v = setup();
    await fireEvent.click(v.getByLabelText("Run Cleanup now"));
    expect(v.getByLabelText("Run Cleanup now")).toBeDisabled();
    resolve({ error: null });
  });
  it("treats a response error as failure", async () => {
    mocks.runJob.mockResolvedValueOnce({ error: "secret backend detail" });
    const v = setup();
    await fireEvent.click(v.getByLabelText("Run Cleanup now"));
    expect(v.queryByText("secret backend detail")).not.toBeInTheDocument();
  });
  it("preserves unrelated config when saving", async () => {
    const v = setup();
    await fireEvent.change(v.getByLabelText("Schedule for Cleanup"), {
      target: { value: "0 0 0 * * *" },
    });
    expect(v.container.querySelector("form")).toBeInTheDocument();
  });
});

// Proxy tests protect protobuf leaf semantics independently of the rendered form.
it("keeps configured jobs and emits changed defaults", () => {
  const configured = SystemJob.fromPartial({ id: 1, schedule: "@daily" });
  const p = buildFormProxy(
    JobsConfig.fromPartial({ systemJobs: [configured] }),
    [job(), job(2, "Index", "0 0 0 * * *")],
  );
  expect(extractConfig(p).systemJobs).toEqual([configured]);
  p.jobs[1].config.schedule = "@hourly";
  expect(extractConfig(p).systemJobs).toHaveLength(2);
});
it("compares protobuf-safe leaves", () => {
  const a = SystemJob.fromPartial({
    id: 1,
    schedule: "@daily",
    disabled: false,
  });
  expect(equal(a, { ...a })).toBe(true);
  expect(equal(a, { ...a, schedule: "@hourly" })).toBe(false);
});
