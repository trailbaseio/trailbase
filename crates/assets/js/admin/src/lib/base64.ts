import { urlSafeBase64Encode } from "trailbase";
import { showToast } from "@/components/ui/toast";

export class UrlSafeBase64EncoderStream extends TransformStream {
  constructor(maxChunk: number = 1000) {
    const max = Math.floor(maxChunk / 3) * 3;
    let buffer = new Uint8Array(0);

    super({
      transform(chunk, controller) {
        // Combine previous leftovers with latest chunk.
        let combined = new Uint8Array(buffer.length + chunk.length);
        combined.set(buffer);
        combined.set(chunk, buffer.length);

        while (true) {
          // Process in multiples of 3 bytes (base64 encodes 3 bytes -> 4 chars).
          const processLength = Math.min(
            Math.floor(combined.length / 3) * 3,
            max,
          );
          if (processLength <= 0) {
            break;
          }

          const toEncode = combined.slice(0, processLength);
          const encoded = urlSafeBase64Encode(toEncode);
          controller.enqueue(encoded);

          // Chomp off the front.
          combined = combined.slice(processLength);
        }

        // Keep remaining bytes for next chunk
        buffer = combined;
      },

      flush(controller) {
        // Flush any remaining bytes, which may also add padding if length < 3.
        if (buffer.length > 0) {
          const encoded = urlSafeBase64Encode(buffer);
          controller.enqueue(encoded);
        }
      },
    });
  }
}

/// We're stream encoding files, since browsers have strict limits.
/// Either way, this is lazy. Ideally we'd use multipart uploads instead.
export async function urlSafeBase64EncodeStream(
  stream: ReadableStream<Uint8Array>,
): Promise<string> {
  const reader = stream
    .pipeThrough(new UrlSafeBase64EncoderStream(12 * 1024))
    .getReader();

  let warned = false;
  const maybeWarn = (bytes: number) => {
    // QUESTION: At what size should we best emit a warning if writtenBytes gets large?
    if (!warned && bytes > WARN_LIMIT) {
      showToast({
        title: `Large input. You may run into browser and/or request size limits.`,
        variant: "warning",
      });
      warned = true;
    }
  };

  let bytes = 0;
  const chunks = [];
  while (true) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }

    chunks.push(value);

    bytes += value.length;
    maybeWarn(bytes);
  }

  return chunks.join("");
}

const WARN_LIMIT = 5 * 1024 * 1024;
