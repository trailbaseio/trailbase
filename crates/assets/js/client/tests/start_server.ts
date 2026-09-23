import type { TestProject } from "vitest/node";
import { cwd } from "node:process";
import { existsSync } from "node:fs";
import type { ChildProcess } from "node:child_process";
import { resolve } from "node:path";
import spawn from "nano-spawn";

import { serverAddress, serverPort, envVarSet } from "./util";

const useWebSocket = envVarSet("USE_WS");
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function initTrailBase(): Promise<{ subprocess: ChildProcess | null }> {
  if (serverPort() === 4000) {
    console.info("Skipping server-startup, relying on external instance.");
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

  const features = useWebSocket ? ["--features=ws"] : [];
  await spawn("cargo", ["build", ...features], { cwd: root });

  const args = [
    "run",
    ...features,
    "--",
    "--data-dir=client/testfixture",
    `--public-url=http://${serverAddress()}`,
    "run",
    `--address=${serverAddress()}`,
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
      const response = await fetch(`http://${serverAddress()}/api/healthcheck`);
      if (response.ok) {
        return { subprocess: child };
      }

      console.log(await response.text());
    } catch {
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

let server: ChildProcess | null = null;

export async function setup(_project: TestProject) {
  if (process.argv.includes("list")) {
    return; // Skip server startup when listing tests
  }

  const { subprocess } = await initTrailBase();
  server = subprocess;
}

export async function teardown() {
  if (!server) {
    return;
  }

  if (server.exitCode === null) {
    // Still running
    console.info("Shutting down TrailBase");
    server.kill();
  } else {
    // Otherwise TrailBase terminated. Log output to provide a clue as to why.
    const { stderr, stdout } = server;
    console.error(stdout);
    console.error(stderr);
  }
}
