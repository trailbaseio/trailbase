/* eslint-disable @typescript-eslint/no-unused-vars */

import { createVitest } from "vitest/node";
import { cwd } from "node:process";
import { existsSync } from "node:fs";
import type { ChildProcess } from "node:child_process";
import { join, resolve } from "node:path";
import spawn from "nano-spawn";

import { ADDRESS, PORT, USE_WS } from "./constants";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function initTrailBase(): Promise<{ subprocess: ChildProcess | null }> {
  if (PORT === 4000) {
    // Rely on externally bootstrapped instance.
    return { subprocess: null };
  }

  const pwd = cwd();
  if (!pwd.endsWith("client")) {
    throw Error(`Unexpected CWD: ${pwd}`);
  }

  const root = resolve(__dirname, "../../../../..");
  if (!existsSync(`${root}/Cargo.lock`)) {
    throw new Error(root);
  }

  const features = USE_WS ? ["--features=ws"] : [];
  await spawn("cargo", ["build", ...features], { cwd: root });

  const args = [
    "run",
    ...features,
    "--",
    "--data-dir=client/testfixture",
    `--public-url=http://${ADDRESS}`,
    "run",
    `--address=${ADDRESS}`,
    "--runtime-threads=1",
  ];

  const subprocess = spawn("cargo", args, {
    cwd: root,
    stdout: process.stdout,
    stderr: process.stdout,
  });

  // NOTE: debug builds of trail loading JS-WASM can take a long time.
  for (let i = 0; i < 300; ++i) {
    const child = await subprocess.nodeChildProcess;
    if ((child.exitCode ?? 0) > 0) {
      break;
    }

    try {
      const response = await fetch(`http://${ADDRESS}/api/healthcheck`);
      if (response.ok) {
        return { subprocess: child };
      }

      console.log(await response.text());
    } catch (err) {
      console.info("Waiting for TrailBase to become healthy");
    }

    await sleep(500);
  }

  const child = await subprocess.nodeChildProcess;
  child.kill();

  const result = await subprocess;
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
