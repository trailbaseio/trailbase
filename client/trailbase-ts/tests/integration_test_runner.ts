/* eslint-disable @typescript-eslint/no-unused-vars */

import { createVitest } from "vitest/node";
import { cwd } from "node:process";
import { execa, type Subprocess } from "execa";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const port: number = 4005;

async function initTrailBase(): Promise<{ subprocess: Subprocess }> {
  const pwd = cwd();
  if (!pwd.endsWith("trailbase-ts")) {
    throw Error(`Unxpected CWD: ${pwd}`);
  }

  const build = await execa`cargo build`;
  if (build.failed) {
    console.error("STDOUT:", build.stdout);
    console.error("STDERR:", build.stderr);
    throw Error("cargo build failed");
  }

  const subprocess = execa`cargo run -- --data-dir ../testfixture run -a 127.0.0.1:${port} --js-runtime-threads 2`;

  for (let i = 0; i < 100; ++i) {
    if ((subprocess.exitCode ?? 0) > 0) {
      break;
    }

    try {
      const response = await fetch(`http://127.0.0.1:${port}/api/healthcheck`);
      if (response.ok) {
        return { subprocess };
      }

      console.log(await response.text());
    } catch (err) {
      console.info("Waiting for TrailBase to become healthy");
    }

    await sleep(500);
  }

  subprocess.kill();

  const result = await subprocess;
  console.error("EXIT:", result.exitCode);
  console.error("STDOUT:", result.stdout);
  console.error("STDERR:", result.stderr);

  throw Error("Failed to start TrailBase");
}

const { subprocess } = await initTrailBase();

const ctx = await createVitest("test", {
  watch: false,
  environment: "jsdom",
  include: ["tests/integration/*"],
});
await ctx.start();
await ctx.close();

if (subprocess.exitCode === null) {
  // Still running
  subprocess.kill();
} else {
  // Otherwise TrailBase terminated. Log output to provide a clue as to why.
  const { stderr, stdout } = subprocess;
  console.error(stdout);
  console.error(stderr);
}
