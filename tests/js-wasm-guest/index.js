import {
  ResponseOutparam,
  OutgoingBody,
  OutgoingResponse,
  Fields,
} from 'wasi:http/types@0.2.3';

function writeResponse(responseOutparam, status, body) {
  const outgoingResponse = new OutgoingResponse(new Fields());

  // Access the outgoing response body
  let outgoingBody = outgoingResponse.body();
  {
    // Create a stream for the response body
    let outputStream = outgoingBody.write();
    outputStream.blockingWriteAndFlush(
      new Uint8Array(new TextEncoder().encode(body))
    );
    outputStream[Symbol.dispose]();
  }

  outgoingResponse.setStatusCode(status);
  OutgoingBody.finish(outgoingBody, undefined);
  ResponseOutparam.set(responseOutparam, { tag: 'ok', val: outgoingResponse });
}

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

const incomingHandler = {
  handle: async function(incomingRequest, responseOutparam) {
    const path = incomingRequest.pathWithQuery();
    console.log(`HTTP request: ${path}`);

    switch (path) {
      case '/fibonacci':
        writeResponse(responseOutparam, 200, fibonacci(40));
        break;
      default:
        writeResponse(responseOutparam, 200, 'Hello from Javascript!\n');
        break;
    }
  }
};

// addEventListener("fetch", (event) =>
//   event.respondWith(
//     (async () => {
//       return new Response("Hello World");
//     })(),
//   ),
// );

const initEndpoint = {
  init: async function() {
    return {
      httpHandlers: [
        ['get', '/fibonacci'],
        ['get', '/wasm/{placeholder}'],
      ],
      jobHandlers: [],
    };
  }
};

export {
  incomingHandler,
  initEndpoint,
}
