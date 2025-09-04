const intrinsicSetInterval = globalThis.setInterval;
const intrinsicClearInterval = globalThis.clearInterval;

const intrinsicSetTimeout = globalThis.setTimeout;
const intrinsicClearTimeout = globalThis.clearTimeout;

type Timeout = number;
// eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
type TimerHandler = Function | string;

const PENDING_INTERVALS: Map<Timeout, PromiseWithResolvers<void>> = new Map();
const PENDING_TIMEOUTS: Map<Timeout, PromiseWithResolvers<void>> = new Map();

export async function awaitPendingTimers() {
  await Promise.all([...PENDING_INTERVALS.values()].map((p) => p.promise));
  await Promise.all([...PENDING_TIMEOUTS.values()].map((p) => p.promise));
}

function setTimeout<TArgs>(
  handler: TimerHandler,
  ms?: number,
  ...args: TArgs[]
): Timeout {
  if (typeof handler === "string") {
    // Avoid a dependency on `eval`.
    throw new Error("string handlers not supported");
  }

  const promiseWithResolvers = Promise.withResolvers<void>();

  const handle: Timeout = intrinsicSetTimeout(() => {
    handler(...args);
    promiseWithResolvers.resolve();
  }, ms);

  PENDING_TIMEOUTS.set(handle, promiseWithResolvers);

  return handle;
}

globalThis.setTimeout = setTimeout;

function clearTimeout(id: number | undefined): void {
  if (id !== undefined) {
    PENDING_TIMEOUTS.get(id)?.resolve();
    PENDING_TIMEOUTS.delete(id);
  }
  intrinsicClearTimeout(id);
}

globalThis.clearTimeout = clearTimeout;

function setInterval<TArgs>(
  handler: TimerHandler,
  ms?: number,
  ...args: TArgs[]
): Timeout {
  if (typeof handler === "string") {
    // Avoid a dependency on `eval`.
    throw new Error("string handlers not supported");
  }

  const handle: Timeout = intrinsicSetInterval(handler, ms, ...args);
  PENDING_INTERVALS.set(handle, Promise.withResolvers());
  return handle;
}

globalThis.setInterval = setInterval;

function clearInterval(id: number | undefined): void {
  if (id !== undefined) {
    PENDING_INTERVALS.get(id)?.resolve();
    PENDING_INTERVALS.delete(id);
  }
  intrinsicClearInterval(id);
}

globalThis.clearInterval = clearInterval;

/// Install a periodic callback (compatible from JS runtime).
export function addPeriodicCallback(
  ms: number,
  cb: (cancel: () => void) => void,
): () => void {
  const handle = setInterval(() => {
    cb(() => clearInterval(handle));
  }, ms);

  return () => clearInterval(handle);
}
