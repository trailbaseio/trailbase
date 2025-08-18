import {
  IncomingRequest,
  ResponseOutparam,
  OutgoingBody,
  OutgoingResponse,
  Fields,
} from 'wasi:http/types@0.2.3';
import { initEndpoint as init } from "@wit";

import { listDirectories as _ } from "./runtime";

function encodeBytes(body: string): Uint8Array {
  return new Uint8Array(new TextEncoder().encode(body));
}

function writeResponse(responseOutparam: ResponseOutparam, status: number, body: string) {
  const outgoingResponse = new OutgoingResponse(new Fields());

  // Access the outgoing response body
  let outgoingBody = outgoingResponse.body();
  {
    // Create a stream for the response body
    let outputStream = outgoingBody.write();
    outputStream.blockingWriteAndFlush(encodeBytes(body));
    // @ts-ignore: This is required in order to dispose the stream before we return
    outputStream[Symbol.dispose]();
  }

  outgoingResponse.setStatusCode(status);
  OutgoingBody.finish(outgoingBody, undefined);
  ResponseOutparam.set(responseOutparam, { tag: 'ok', val: outgoingResponse });
}

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

export const incomingHandler = {
  handle: async function(req: IncomingRequest, resp: ResponseOutparam) {
    const path = req.pathWithQuery();
    console.log(`HTTP request: ${path}`);

    switch (path) {
      case '/fibonacci':
        writeResponse(resp, 200, fibonacci(40).toString());
        break;
      default:
        writeResponse(resp, 200, 'Hello from Javascript!\n');
        break;
    }
  }
};

export const initEndpoint = {
  init: function(): init.InitResult {
    return {
      httpHandlers: [
        ['get', '/fibonacci'],
      ],
      jobHandlers: [],
    };
  },
};
