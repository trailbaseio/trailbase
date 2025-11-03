/* eslint-disable @typescript-eslint/no-unused-vars */

import { createVitest } from "vitest/node";
import { cwd } from "node:process";
import { join } from "node:path";
import { execa, type Subprocess } from "execa";

import { ADDRESS, PORT } from "./constants";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function initTrailBase(): Promise<{ subprocess: Subprocess | null }> {
  if (PORT === 4000) {
    // Rely on externally bootstrapped instance.
    return { subprocess: null };
  }

  const pwd = cwd();
  if (!pwd.endsWith("client")) {
    throw Error(`Unexpected CWD: ${pwd}`);
  }

  const root = join(pwd, "..", "..", "..", "..");

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
  })`cargo run -- --data-dir client/testfixture --public-url http://${ADDRESS} run -a ${ADDRESS} --runtime-threads 1`;

  // NOTE: debug builds of trail loading JS-WASM can take a long time.
  for (let i = 0; i < 300; ++i) {
    if ((subprocess.exitCode ?? 0) > 0) {
      break;
    }

    try {
      const response = await fetch(`http://${ADDRESS}/api/healthcheck`);
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

{
  const ctx = await createVitest("test", {
    watch: false,
    environment: "jsdom",
    include: ["tests/integration/*"],
    exclude: [
      "tests/integration/auth_integration.test.ts",
      "tests/integration/v8_integration.test.ts",
    ],
  });

  await ctx.start();
  await ctx.close();
}

{
  const ctx = await createVitest("test", {
    watch: false,
    environment: "node",
    include: [
      "tests/integration/auth_integration.test.ts",
      "tests/integration/v8_integration.test.ts",
    ],
  });

  await ctx.start();
  await ctx.close();
}

if (subprocess !== null) {
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
}
