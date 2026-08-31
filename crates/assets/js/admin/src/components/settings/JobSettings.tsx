import {
  Match,
  Switch,
  Show,
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

const cronAliases = new Set([
  "@yearly",
  "@monthly",
  "@weekly",
  "@daily",
  "@hourly",
]);
export function validCron(value: string) {
  const normalized = value.trim();
  if (cronAliases.has(normalized)) return true;
  const fields = normalized.split(/\s+/);
  return (
    (fields.length === 6 || fields.length === 7) &&
    fields.every((field) => /^[A-Za-z0-9*?,/-]+$/.test(field))
  );
}
function isValidCronSpec() {
  return {
    onChange: ({ value }: { value: string }) =>
      validCron(value) ? undefined : "Not a valid cron spec",
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
    if (job.id !== undefined)
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
function jobsSignature(jobs: Job[]) {
  return jobs
    .map((job) =>
      [
        job.id,
        job.name,
        job.schedule,
        job.enabled,
        job.next?.toString() ?? "",
        job.latest?.map((value) => value?.toString() ?? "").join("|") ?? "",
      ].join("\u001f"),
    )
    .sort()
    .join("\u001e");
}
function configSignature(config: Config) {
  return Array.from(Config.encode(config).finish()).join(",");
}
function jobsConfigSignature(config: JobsConfig) {
  return Array.from(JobsConfig.encode(config).finish()).join(",");
}
export function mergeJobs(
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
  for (const x of submitted.systemJobs) {
    if (latest.systemJobs.some((r) => r.id === x.id)) continue;
    const old = base.get(x.id);
    if (!old || !equal(x, old)) merged.push(x);
  }
  return { systemJobs: merged };
}
function safeDate(value: unknown) {
  try {
    const seconds = BigInt(typeof value === "bigint" ? value : String(value));
    const millis = seconds * 1000n;
    const min = BigInt(-8_640_000_000_000_000);
    const max = BigInt(8_640_000_000_000_000);
    if (millis < min || millis > max) return null;
    const d = new Date(Number(millis));
    return Number.isNaN(d.getTime()) ? null : d;
  } catch {
    return null;
  }
}
export function formatDurationMillis(value: unknown) {
  let milliseconds: bigint;
  if (typeof value === "bigint") milliseconds = value;
  else if (typeof value === "number" && Number.isSafeInteger(value)) {
    milliseconds = BigInt(value);
  } else if (typeof value === "string" && /^-?\d+$/.test(value)) {
    milliseconds = BigInt(value);
  } else return "—";

  const sign = milliseconds < 0n ? "-" : "";
  const absolute = milliseconds < 0n ? -milliseconds : milliseconds;
  const seconds = absolute / 1000n;
  const millis = (absolute % 1000n)
    .toString()
    .padStart(3, "0")
    .replace(/0+$/, "");
  return `${sign}${seconds}${millis ? `.${millis}` : ""}s`;
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
  let baseline = untrack(() => buildFormProxy(props.config.jobs, props.jobs));
  let latestConfig = untrack(() => cloneConfig(props.config)),
    latestJobs = untrack(() => cloneJobs(props.jobs)),
    lastConfig = untrack(() => configSignature(props.config)),
    lastJobs = untrack(() => jobsSignature(props.jobs)),
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
        baselineAtSubmit = {
          systemJobs: baseline.jobs.map((entry) => ({ ...entry.config })),
        },
        configRevisionAtSubmit = configSignature(latestConfig),
        jobsRevisionAtSubmit = jobsSignature(latestJobs);
      try {
        const merged = mergeJobs(
          submittedJobs,
          baselineAtSubmit,
          latestConfig.jobs ?? JobsConfig.create(),
        );
        const save = cloneConfig(configAtSubmit);
        save.jobs = merged;
        await setConfig({ client: queryClient, config: save, throw: true });
        const refreshed = await props.refetchJobs();
        if (
          refreshed &&
          typeof refreshed === "object" &&
          "isError" in refreshed &&
          refreshed.isError
        ) {
          throw new Error("Jobs refresh failed");
        }
        if (!active) return;
        const refreshedDuringSave =
          configRevisionAtSubmit !== configSignature(latestConfig) ||
          jobsRevisionAtSubmit !== jobsSignature(latestJobs);
        const finalJobs = refreshedDuringSave
          ? mergeJobs(
              submittedJobs,
              baselineAtSubmit,
              latestConfig.jobs ?? JobsConfig.create(),
            )
          : merged;
        latestConfig = refreshedDuringSave ? cloneConfig(latestConfig) : save;
        latestConfig.jobs = finalJobs;
        baseline = buildFormProxy(finalJobs, latestJobs);
        const stillEdited =
          jobsConfigSignature(extractConfig(form.state.values)) !==
          jobsConfigSignature(submittedJobs);
        if (!stillEdited) form.reset(baseline);
        setSubmitError(false);
        props.postSubmit(stillEdited);
      } catch {
        if (active) setSubmitError(true);
      }
    },
  }));
  const dirty = () =>
    jobsConfigSignature(extractConfig(formValues())) !==
    jobsConfigSignature(extractConfig(baseline));
  const formValues = form.useSelector((state) => state.values);
  createEffect(() => props.setDirty(dirty()));
  createEffect(() => {
    const incoming = props.config,
      encoded = configSignature(incoming),
      incomingJobs = jobsSignature(props.jobs);
    const changed = encoded !== lastConfig || incomingJobs !== lastJobs;
    if (!changed) return;
    const wasDirty = untrack(dirty);
    const previousRemote = buildFormProxy(latestConfig.jobs, latestJobs);
    lastConfig = encoded;
    lastJobs = incomingJobs;
    latestConfig = cloneConfig(incoming);
    latestJobs = cloneJobs(props.jobs);
    const incomingBaseline = buildFormProxy(incoming.jobs, props.jobs);
    if (!wasDirty) {
      baseline = incomingBaseline;
      form.reset(baseline);
    } else {
      const current = form.state.values.jobs;
      baseline = incomingBaseline;
      form.reset(incomingBaseline);
      for (const [index, entry] of baseline.jobs.entries()) {
        const previous = current.find(
          (item) => item.config.id === entry.config.id,
        );
        const old = previousRemote.jobs.find(
          (item) => item.config.id === entry.config.id,
        );
        if (!previous || !old) continue;
        if (previous.config.schedule !== old.config.schedule)
          form.setFieldValue(
            `jobs[${index}].config.schedule`,
            previous.config.schedule,
          );
        if (previous.config.disabled !== old.config.disabled)
          form.setFieldValue(
            `jobs[${index}].config.disabled`,
            previous.config.disabled,
          );
      }
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
      const refreshed = await props.refetchJobs();
      if (
        refreshed &&
        typeof refreshed === "object" &&
        "isError" in refreshed &&
        refreshed.isError
      ) {
        throw new Error("Jobs refresh failed");
      }
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
          <p class="text-muted-foreground text-sm">
            Schedules use six or seven components in this order: second, minute,
            hour, day of month, month, day of week, and optional year. Supported
            aliases are exactly <code>@yearly</code>, <code>@monthly</code>,
            <code>@weekly</code>, <code>@daily</code>, and <code>@hourly</code>.
          </p>
          {submitError() && (
            <div role="alert">Unable to save job settings.</div>
          )}
          {runError() && <div role="alert">Unable to run job.</div>}
          <div class="overflow-x-auto rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Schedule</TableHead>
                  <TableHead>Next Run</TableHead>
                  <TableHead>Last Run</TableHead>
                  <TableHead>Enabled</TableHead>
                  <TableHead>Action</TableHead>
                </TableRow>
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
                                  <TextField onChange={f().handleChange}>
                                    <TextFieldInput
                                      type="text"
                                      aria-label={`Schedule for ${proxy().job?.name ?? proxy().config.id}`}
                                      value={f().state.value}
                                      onBlur={f().handleBlur}
                                      autocomplete="off"
                                    />
                                  </TextField>
                                  <FieldInfo field={f()} />
                                </>
                              )}
                            </form.Field>
                          </TableCell>
                          <TableCell>{time(proxy().job?.next)}</TableCell>
                          <TableCell>
                            <Show when={proxy().job?.latest} fallback="—">
                              {(latest) => (
                                <span
                                  class={
                                    latest()[2] ? "text-error-foreground" : ""
                                  }
                                  title={latest()[2] ? "Job error" : undefined}
                                >
                                  {time(latest()[0])} (
                                  {formatDurationMillis(latest()[1])})
                                  {latest()[2] ? " — error" : ""}
                                </span>
                              )}
                            </Show>
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
        canSubmit={
          form.state.canSubmit &&
          formValues().jobs.every((entry) =>
            validCron(entry.config.schedule ?? ""),
          )
        }
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
    <Switch fallback={<div role="status">Loading jobs settings…</div>}>
      <Match when={jobList.isLoading || config.isLoading}>
        <div role="status">Loading jobs settings…</div>
      </Match>
      <Match when={jobList.isError}>
        <div role="alert">Unable to load jobs.</div>
      </Match>
      <Match when={config.isError}>
        <div role="alert">Unable to load configuration.</div>
      </Match>
      <Match when={jobList.isSuccess && config.data?.config}>
        <JobSettingsImpl
          {...props}
          config={config.data!.config!}
          jobs={jobList.data?.jobs ?? []}
          refetchJobs={() =>
            queryClient.invalidateQueries(
              { queryKey: listJobsKey },
              { throwOnError: true },
            )
          }
        />
      </Match>
    </Switch>
  );
}
