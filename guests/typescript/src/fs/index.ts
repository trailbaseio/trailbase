import {
  getDirectories,
  type Descriptor,
} from "wasi:filesystem/preopens@0.2.3";
import type { PathFlags } from "wasi:filesystem/types@0.2.3";

// Override setInterval/setTimeout.
import "../timer";

export function readFileSync(path: string): Uint8Array {
  const root = getDirectories().find(([_, path]) => path === "/")?.[0];
  if (!root) {
    throw new Error("Missing '/'");
  }

  const segments = path.split("/");
  let descriptor: Descriptor = root;

  for (const [i, segment] of segments.entries()) {
    if (i === 0) {
      if (segment !== "") {
        throw new Error(`Only absolute paths, got: ${path}`);
      }
      continue;
    }

    const last = i == segments.length - 1;
    const flags: PathFlags = {
      symlinkFollow: false,
    };

    descriptor = descriptor.openAt(
      flags,
      segment,
      last ? {} : { directory: true },
      { read: true },
    );

    if (last) {
      const MAX = BigInt(1024 * 1024);
      const buffer = [];

      while (true) {
        const [bytes, eof] = descriptor.read(MAX, BigInt(buffer.length));
        buffer.push(...bytes);

        if (eof) {
          break;
        }
      }

      return Uint8Array.from(buffer);
    }
  }

  throw new Error("not found");
}
