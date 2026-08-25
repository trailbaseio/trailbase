export type JobHandlerType = () => void | Promise<void>;

export type JobHandlerInterface = {
  /// Unique name of the job.
  name: string;
  /// Cron spec.
  spec: string;
  /// Timeout in milliseconds.
  timeout?: number;
  /// The callback doing the work.
  handler: JobHandlerType;
};

export class JobHandler implements JobHandlerInterface {
  constructor(
    public readonly name: string,
    public readonly spec: string,
    public readonly handler: JobHandlerType,
    public readonly timeout?: number,
  ) {
    validateSpec(this.spec);
  }

  static daily(name: string, handler: JobHandlerType): JobHandler {
    return new JobHandler(name, "@daily", handler, 24 * 3600 * 1000);
  }

  static hourly(name: string, handler: JobHandlerType): JobHandler {
    return new JobHandler(name, "@hourly", handler, 3600 * 1000);
  }

  static minutely(name: string, handler: JobHandlerType): JobHandler {
    const second: number = 5;
    return new JobHandler(name, `${second} * * * * *`, handler, 60 * 1000);
  }
}

function validateSpec(spec: string) {
  switch (spec) {
    case "@hourly":
    case "@daily":
    case "@weekly":
    case "@monthly":
    case "@yearly":
      return;
    default: {
      const components = spec.trim().split(" ");
      switch (components.length) {
        case 6:
        case 7:
          return;
        default:
          throw new Error(`Unepxected number of components: ${spec}`);
      }
    }
  }
}
