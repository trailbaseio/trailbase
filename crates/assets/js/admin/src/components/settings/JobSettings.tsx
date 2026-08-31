import {
  Match,
  Switch,
  Index,
  createEffect,
  createSignal,
  onCleanup,
  untrack,
} from "solid-js";
import { createForm } from "@tanstack/solid-form";
import { useQueryClient, useQuery } from "@tanstack/solid-query";
import { TbOutlinePlayerPlay } from "solid-icons/tb";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { IconButton } from "@/components/IconButton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { TextField, TextFieldInput } from "@/components/ui/text-field";
import { FieldInfo, type FieldApiT } from "@/components/FormFields";
import { SettingsFormActions } from "@/components/settings/SettingsFormActions";
import { Config, JobsConfig, SystemJob } from "@proto/config";
import { createConfigQuery, setConfig } from "@/lib/api/config";
import { listJobs, runJob } from "@/lib/api/jobs";
import { showToast } from "@/components/ui/toast";
import type { Job } from "@bindings/Job";

const cronRegex =
  /^(@(yearly|monthly|weekly|daily|hourly))|((((\d+,)+\d+|(\d+(\/|-)\d+)|\d+|\*)\s*){6,7})$/;
function isValidCronSpec() {
  return {
    onChange: ({ value }: { value: string }) =>
      cronRegex.test(value) ? undefined : "Not a valid cron spec",
  };
}

type JobProxy = {
  default: boolean;
  initialConfig: SystemJob;
  config: SystemJob;
  job?: Job;
};
type FormProxy = { jobs: JobProxy[] };
export function equal(a: SystemJob, b: SystemJob) {
  return (
    a.id === b.id && a.schedule === b.schedule && a.disabled === b.disabled
  );
}
export function buildFormProxy(
  config: JobsConfig | undefined,
  jobs: Job[],
): FormProxy {
  const result = new Map<number, JobProxy>();
  for (const job of config?.systemJobs ?? [])
    if (job.id)
      result.set(job.id, {
        default: false,
        initialConfig: job,
        config: { ...job },
      });
  for (const job of jobs) {
    const d = { id: job.id, schedule: job.schedule, disabled: !job.enabled };
    const entry: JobProxy = result.get(job.id) ?? {
      default: true,
      initialConfig: d,
      config: { ...d },
    };
    entry.job = job;
    result.set(job.id, entry);
  }
  return {
    jobs: [...result.values()].sort(
      (a, b) => (a.config.id ?? 0) - (b.config.id ?? 0),
    ),
  };
}
export function extractConfig(proxy: FormProxy): JobsConfig {
  return {
    systemJobs: proxy.jobs
      .filter((e) => !e.default || !equal(e.initialConfig, e.config))
      .map((e) => e.config),
  };
}
const cloneConfig = (c: Config) => Config.decode(Config.encode(c).finish());
const cloneJobs = (j: Job[]) => j.map((x) => ({ ...x }));
function leafChanged(a: SystemJob, b: SystemJob) {
  return (
    a.id !== b.id || a.schedule !== b.schedule || a.disabled !== b.disabled
  );
}
function mergeJobs(
  submitted: JobsConfig,
  baseline: JobsConfig,
  latest: JobsConfig,
): JobsConfig {
  const local = new Map(submitted.systemJobs.map((x) => [x.id, x]));
  const base = new Map(baseline.systemJobs.map((x) => [x.id, x]));
  const merged = latest.systemJobs.map((remote) => {
    const mine = local.get(remote.id),
      old = base.get(remote.id);
    if (!mine || !old) return remote;
    return {
      ...remote,
      ...(mine.schedule !== old.schedule ? { schedule: mine.schedule } : {}),
      ...(mine.disabled !== old.disabled ? { disabled: mine.disabled } : {}),
    };
  });
  for (const x of submitted.systemJobs)
    if (!latest.systemJobs.some((r) => r.id === x.id)) merged.push(x);
  return { systemJobs: merged };
}
function safeDate(value: unknown) {
  try {
    const s = typeof value === "bigint" ? value.toString() : String(value);
    const t = Number(s);
    if (!Number.isSafeInteger(t) || t <= 0) return null;
    const d = new Date(t * 1000);
    return Number.isNaN(d.getTime()) ? null : d;
  } catch {
    return null;
  }
}
function time(value: unknown) {
  return safeDate(value)?.toUTCString() ?? "—";
}

function JobSettingsImpl(props: {
  setDirty: (dirty: boolean) => void;
  postSubmit: (dirty?: boolean) => void;
  config: Config;
  jobs: Job[];
  refetchJobs: () => Promise<unknown>;
}) {
  const queryClient = useQueryClient();
  let baseline = buildFormProxy(props.config.jobs, props.jobs);
  let latestConfig = cloneConfig(props.config),
    latestJobs = cloneJobs(props.jobs),
    lastConfig = Config.encode(props.config).finish(),
    active = true;
  const [submitError, setSubmitError] = createSignal(false),
    [runError, setRunError] = createSignal(false),
    [pendingRun, setPendingRun] = createSignal<number>();
  onCleanup(() => {
    active = false;
  });
  const form = createForm(() => ({
    defaultValues: baseline,
    onSubmit: async ({ value }: { value: FormProxy }) => {
      const submitted = {
        jobs: value.jobs.map((e) => ({
          ...e,
          config: { ...e.config },
          initialConfig: { ...e.initialConfig },
        })),
      };
      const submittedJobs = extractConfig(submitted),
        configAtSubmit = cloneConfig(latestConfig),
        baselineAtSubmit = extractConfig(baseline);
      try {
        const merged = mergeJobs(
          submittedJobs,
          baselineAtSubmit,
          latestConfig.jobs ?? JobsConfig.create(),
        );
        const save = cloneConfig(configAtSubmit);
        save.jobs = merged;
        await setConfig({ client: queryClient, config: save, throw: true });
        await props.refetchJobs();
        if (!active) return;
        latestConfig = save;
        baseline = buildFormProxy(merged, latestJobs);
        const stillEdited =
          extractConfig(form.state.values) !== undefined &&
          JSON.stringify(extractConfig(form.state.values)) !==
            JSON.stringify(submittedJobs);
        if (!stillEdited) form.reset(baseline);
        setSubmitError(false);
        props.postSubmit(stillEdited);
      } catch {
        if (active) setSubmitError(true);
      }
    },
  }));
  const dirty = () =>
    JSON.stringify(extractConfig(form.state.values)) !==
    JSON.stringify(extractConfig(baseline));
  form.useStore(() => props.setDirty(dirty()));
  createEffect(() => {
    const incoming = props.config,
      encoded = Config.encode(incoming).finish();
    const changed =
      encoded.length !== lastConfig.length ||
      encoded.some((x, i) => x !== lastConfig[i]) ||
      props.jobs !== latestJobs;
    if (!changed) return;
    const wasDirty = untrack(dirty);
    lastConfig = encoded;
    latestConfig = cloneConfig(incoming);
    latestJobs = cloneJobs(props.jobs);
    if (!wasDirty) {
      baseline = buildFormProxy(incoming.jobs, props.jobs);
      form.reset(baseline);
    }
  });
  const reset = () => {
    form.reset(baseline);
    setSubmitError(false);
    setRunError(false);
  };
  const run = async (id: number) => {
    if (pendingRun() !== undefined) return;
    setPendingRun(id);
    setRunError(false);
    try {
      const result = await runJob({ id });
      if (result.error) throw new Error("job failed");
      await props.refetchJobs();
      if (active) showToast({ title: "Job started", variant: "success" });
    } catch {
      if (active) {
        setRunError(true);
        showToast({
          title: "Job failed",
          description: "The job could not be started.",
          variant: "error",
        });
      }
    } finally {
      if (active) setPendingRun(undefined);
    }
  };
  return (
    <form
      method="dialog"
      onSubmit={(e) => {
        e.preventDefault();
        form.handleSubmit();
      }}
    >
      <Card>
        <CardHeader>
          <h2>Periodic Jobs</h2>
        </CardHeader>
        <CardContent class="flex flex-col gap-4">
          <p class="text-sm">
            The following jobs, when enabled, execute periodically in the
            background. This may include default system jobs, such as session
            cleanup, as well as jobs registered by WASM components.
          </p>
          {submitError() && (
            <div role="alert">Unable to save job settings.</div>
          )}
          {runError() && <div role="alert">Unable to run job.</div>}
          <div class="overflow-x-auto rounded-md border">
            <Table>
              <TableHeader>
                <TableHead>Name</TableHead>
                <TableHead>Schedule</TableHead>
                <TableHead>Next Run</TableHead>
                <TableHead>Last Run</TableHead>
                <TableHead>Enabled</TableHead>
                <TableHead>Action</TableHead>
              </TableHeader>
              <TableBody>
                <form.Field name="jobs" mode="array">
                  {(field) => (
                    <Index each={field().state.value}>
                      {(proxy, i) => (
                        <TableRow>
                          <TableCell>
                            {proxy().job?.name ?? `Job ${proxy().config.id}`}
                          </TableCell>
                          <TableCell>
                            <form.Field
                              name={`jobs[${i}].config.schedule`}
                              validators={isValidCronSpec()}
                            >
                              {(f: () => FieldApiT<string | undefined>) => (
                                <>
                                  <TextField>
                                    <TextFieldInput
                                      type="text"
                                      aria-label={`Schedule for ${proxy().job?.name ?? proxy().config.id}`}
                                      value={f().state.value}
                                      onBlur={f().handleBlur}
                                      onChange={(e) =>
                                        f().handleChange(
                                          (e.target as HTMLInputElement).value,
                                        )
                                      }
                                    />
                                  </TextField>
                                  <FieldInfo field={f()} />
                                </>
                              )}
                            </form.Field>
                          </TableCell>
                          <TableCell>{time(proxy().job?.next)}</TableCell>
                          <TableCell>
                            {(() => {
                              const l = proxy().job?.latest;
                              return l ? (
                                <span
                                  class={l[2] ? "text-error-foreground" : ""}
                                  title={l[2] ? "Job error" : undefined}
                                >
                                  {time(l[0])} ({Number(l[1]) / 1000}s)
                                  {l[2] ? " — error" : ""}
                                </span>
                              ) : (
                                "—"
                              );
                            })()}
                          </TableCell>
                          <TableCell>
                            <form.Field name={`jobs[${i}].config.disabled`}>
                              {(f: () => FieldApiT<boolean>) => (
                                <Checkbox
                                  aria-label={`Enabled for ${proxy().job?.name ?? proxy().config.id}`}
                                  checked={!f().state.value}
                                  onChange={(v) => f().handleChange(!v)}
                                />
                              )}
                            </form.Field>
                          </TableCell>
                          <TableCell>
                            <IconButton
                              aria-label={`Run ${proxy().job?.name ?? proxy().config.id} now`}
                              tooltip="Run now"
                              type="button"
                              disabled={pendingRun() !== undefined}
                              onClick={() => run(proxy().job?.id ?? 0)}
                            >
                              {pendingRun() === proxy().job?.id ? (
                                "Running…"
                              ) : (
                                <TbOutlinePlayerPlay />
                              )}
                            </IconButton>
                          </TableCell>
                        </TableRow>
                      )}
                    </Index>
                  )}
                </form.Field>
              </TableBody>
            </Table>
          </div>
          <Switch fallback={null}>
            <Match when={!props.jobs.length}>
              <p>No jobs configured.</p>
            </Match>
          </Switch>
        </CardContent>
      </Card>
      <SettingsFormActions
        dirty={dirty()}
        canSubmit={form.state.canSubmit}
        isSubmitting={form.state.isSubmitting}
        onReset={reset}
      />
    </form>
  );
}
const listJobsKey = ["admin", "jobs"];
export function JobSettings(props: {
  setDirty: (dirty: boolean) => void;
  postSubmit: (dirty?: boolean) => void;
}) {
  const queryClient = useQueryClient(),
    config = createConfigQuery(),
    jobList = useQuery(() => ({ queryKey: listJobsKey, queryFn: listJobs }));
  return (
    <Switch fallback="Loading…">
      <Match when={jobList.isError}>
        <div role="alert">Unable to load jobs.</div>
      </Match>
      <Match when={config.error}>
        <div role="alert">Unable to load configuration.</div>
      </Match>
      <Match when={jobList.isSuccess && config.data?.config}>
        <JobSettingsImpl
          {...props}
          config={config.data!.config!}
          jobs={jobList.data?.jobs ?? []}
          refetchJobs={() =>
            queryClient.invalidateQueries({ queryKey: listJobsKey })
          }
        />
      </Match>
    </Switch>
  );
}
