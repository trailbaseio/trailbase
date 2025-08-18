// import type { InitResult } from "trailbase-wasm";
import {
  IncomingRequest,
  defineConfig,
  // threadId,
  // writeResponse,
} from "trailbase-wasm";

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

// export const incomingHandler = {
//   handle: async function(req: IncomingRequest, resp: ResponseOutparam) {
//     const path = req.pathWithQuery();
//     console.log(`HTTP request [${threadId()}]: ${path}`);
//
//     switch (path) {
//       case '/fibonacci':
//         writeResponse(resp, 200, fibonacci(40).toString());
//         break;
//       default:
//         writeResponse(resp, 200, 'Hello from Javascript!\n');
//         break;
//     }
//   }
// };
//
// export const initEndpoint = {
//   init: function(): InitResult {
//     return {
//       httpHandlers: [
//         ['get', '/fibonacci'],
//         ['get', '/wasm'],
//       ],
//       jobHandlers: [],
//     };
//   },
// };


const config = defineConfig({
  handlers: [
    {
      path: "/fibonacci",
      method: "get",
      handler: (_req: IncomingRequest): string => {
        return fibonacci(40).toString();
      },
    },
    {
      path: "/wasm",
      method: "get",
      handler: (_req: IncomingRequest): string => {
        return "Hello from Javascript!\n";
      },
    },
  ]
});

// TODO: We should be able to export them in one go.
// export default { ...config };

export const initEndpoint = config.initEndpoint;
export const incomingHandler = config.incomingHandler;
