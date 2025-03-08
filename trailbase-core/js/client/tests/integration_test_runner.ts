/* eslint-disable @typescript-eslint/no-unused-vars */

import { createVitest } from "vitest/node";
import { cwd, chdir } from "node:process";
import { join } from "node:path";
import { execa, type Subprocess } from "execa";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const port: number = 4005;

async function initTrailBase(): Promise<{ subprocess: Subprocess }> {
  const pwd = cwd();
  if (!pwd.endsWith("client")) {
    throw Error(`Unxpected CWD: ${pwd}`);
  }

  const root = join(pwd, "..", "..", "..");

  const build = await execa({ cwd: root })`cargo build`;
  if (build.failed) {
    console.error("STDOUT:", build.stdout);
    console.error("STDERR:", build.stderr);
    throw Error("cargo build failed");
  }

  const subprocess = execa({
    cwd: root,
    stdout: process.stdout,
    stderr: process.stdout,
  })`cargo run -- --data-dir client/testfixture run -a 127.0.0.1:${port} --js-runtime-threads 1`;

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
  console.info("Shutting down TrailBase");
  subprocess.kill();
} else {
  // Otherwise TrailBase terminated. Log output to provide a clue as to why.
  const { stderr, stdout } = subprocess;
  console.error(stdout);
  console.error(stderr);
}
