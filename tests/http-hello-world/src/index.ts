import { Request, defineConfig } from "trailbase-wasm";

function fibonacci(num: number): number {
  switch (num) {
    case 0:
      return 0;
    case 1:
      return 1;
    default:
      return fibonacci(num - 1) + fibonacci(num - 2);
  }
}

const config = defineConfig({
  handlers: [
    {
      path: "/fibonacci",
      method: "get",
      handler: (_req: Request): string => {
        return fibonacci(40).toString();
      },
    },
    {
      path: "/wasm/{placeholder}",
      method: "get",
      handler: (req: Request): string => {
        const path = req.path;

        return `Hello from Javascript ${path}!\n`;
      },
    },
  ]
});

// TODO: We should be able to export them in one go.
// export default { ...config };

export const initEndpoint = config.initEndpoint;
export const incomingHandler = config.incomingHandler;
