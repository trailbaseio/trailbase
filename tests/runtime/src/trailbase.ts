import {
  ResponseOutparam,
  OutgoingBody,
  OutgoingResponse,
  Fields,
} from "wasi:http/types@0.2.3";
import { getRandomBytes as _ } from "wasi:random/random@0.2.3";
import { getDirectories } from "wasi:filesystem/preopens@0.2.3";

export function listDirectories(): string[] {
  return getDirectories().map(([_fd, name]) => {
    return name;
  });
}

function encodeBytes(body: string): Uint8Array {
  return new Uint8Array(new TextEncoder().encode(body));
}

export function writeResponse(
  responseOutparam: ResponseOutparam,
  status: number,
  body: string,
) {
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
  ResponseOutparam.set(responseOutparam, { tag: "ok", val: outgoingResponse });
}

export function test(): string {
  return "test";
}
