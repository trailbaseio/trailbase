import type { TestProject } from "vitest/node";
import { cwd } from "node:process";
import { existsSync } from "node:fs";
import type { ChildProcess } from "node:child_process";
import { resolve } from "node:path";
import spawn from "nano-spawn";

import { envVarSet, serverAddress, serverPort } from "./util";

const useWebSocket = envVarSet("USE_WS");
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function initTrailBase(opts?: {
  timeoutMinutes?: number;
}): Promise<{ subprocess: ChildProcess | null }> {
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

  const features = useWebSocket ? ["--ws"] : [];
  await spawn("python3", ["client/runner.py", "build", ...features], {
    cwd: root,
  });

  const subprocess = spawn(
    "python3",
    [
      "client/runner.py",
      "run",
      ...features,
      `--port=${serverPort()}`,
      "--runtime-threads=1",
    ],
    {
      cwd: root,
      stdout: process.stdout,
      stderr: process.stdout,
    },
  );

  const child = await subprocess.nodeChildProcess;

  const started = new Date().getTime();
  while (true) {
    const code = child.exitCode;
    if ((code ?? 0) > 0) {
      console.error(`Server exited abnormally: ${code}`);
      break;
    }

    try {
      const response = await fetch(`http://${serverAddress()}/api/healthcheck`);
      if (response.ok) {
        return { subprocess: child };
      }
    } catch (err) {
      console.debug(`Health check failed: ${err}`);
    }

    const now = new Date().getTime();
    const elapsedMinutes = (now - started) / 1000 / 60;
    if (elapsedMinutes >= (opts?.timeoutMinutes ?? 12)) {
      break;
    }

    await sleep(500);
  }

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
