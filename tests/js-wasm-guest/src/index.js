import { writeResponse } from "trailbase-wasm";

function fibonacci(num) {
  switch (num) {
    case 0:
      return 0;
    case 1:
      return 1;
    default:
      return fibonacci(num - 1) + fibonacci(num - 2);
  }
}

// addEventListener("fetch", (event) =>
//   event.respondWith(
//     (async () => {
//       return new Response("Hello World");
//     })(),
//   ),
// );

const incomingHandler = {
  handle: async function (incomingRequest, responseOutparam) {
    const path = incomingRequest.pathWithQuery();
    console.log(`HTTP request: ${path}`);

    switch (path) {
      case "/fibonacci":
        writeResponse(responseOutparam, 200, fibonacci(40));
        break;
      default:
        writeResponse(responseOutparam, 200, "Hello from Javascript!\n");
        break;
    }
  },
};

const initEndpoint = {
  init: async function () {
    return {
      httpHandlers: [
        ["get", "/fibonacci"],
        ["get", "/wasm/{placeholder}"],
      ],
      jobHandlers: [],
    };
  },
};

export default {
  initEndpoint,
  incomingHandler,
};
