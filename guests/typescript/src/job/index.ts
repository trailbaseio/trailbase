export type JobHandlerType = () => void | Promise<void>;

export type JobHandlerInterface = {
  name: string;
  spec: string;
  handler: JobHandlerType;
};

export class JobHandler implements JobHandlerInterface {
  constructor(
    public readonly name: string,
    public readonly spec: string,
    public readonly handler: JobHandlerType,
  ) {
    validateSpec(this.spec);
  }

  static hourly(name: string, handler: JobHandlerType): JobHandler {
    return new JobHandler(name, "@hourly", handler);
  }

  static minutely(name: string, handler: JobHandlerType): JobHandler {
    const second: number = 5;
    return new JobHandler(name, `${second} * * * * *`, handler);
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
