import fs from "fs/promises";
import path from "node:path";
import process, { kill } from "node:process";
import { styleText } from "node:util";

import spawn from "nano-spawn";
import { program } from "commander";

const COMPONENT_NAME = "component.wasm";

program
  .description("CLI for hot-reloading a TrailBase dev server")
  .option("--watch-path", "path to watch for changes", "src")
  .option("--depot <TRAILDEPOT>", "path to traildepot", "traildepot")
  .option("--port <PORT>", "TrailBase's port", "4000")
  .option("-p, --pid <PID>", "process to watch")
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  .action(async (options, _command) => {
    const pid = options.pid;
    if (pid) {
      info(
        `Watching pid=${pid}, depot=${options.depot} (cwd=${process.cwd()})`,
      );

      await hotRestart({
        pid: parseInt(pid),
        depotPath: options.depot,
      });
    } else {
      info(`Starting 'trail' and watching: ${options.watchPath}`);
      await startTrailBaseAndHotRestart({
        address: `localhost:${options.port}`,
        watchPath: options.watchPath,
        depotPath: options.depot,
      });
    }
  });

program.parse();

async function startTrailBaseAndHotRestart(opts: {
  address: string;
  watchPath: string;
  depotPath: string;
}) {
  // First build the component, if it doesn't exist. Otherways, we'd be starting `trail`
  // w/o a component leading to no routes being registered.
  await deployComponent({
    depotPath: opts.depotPath,
    alwaysBuild: false,
  });

  const controller = new AbortController();
  const { signal } = controller;

  const trailProcess = spawn(
    "trail",
    [`--data-dir=${opts.depotPath}`, "run", "--dev", `-a=${opts.address}`],
    {
      stdout: "inherit",
      stderr: "inherit",
      killSignal: "SIGKILL",
      signal: signal,
    },
  );

  const pid: number = (await trailProcess.nodeChildProcess).pid!;

  const self = process.argv[1];
  try {
    // Hot-restart by calling our-selves
    await spawn(
      "node",
      [
        "--experimental-strip-types",
        `--watch-path=${opts.watchPath}`,
        self,
        `--pid=${pid}`,
      ],
      { stdout: "inherit" },
    );
  } finally {
    controller.abort();
  }
}

async function deployComponent(opts: {
  depotPath: string;
  alwaysBuild: boolean;
}) {
  const start = Date.now();

  const wasmPath = path.join(opts.depotPath, "wasm");
  const component = path.join(wasmPath, COMPONENT_NAME);
  const exists = await fileExists(component);

  if (opts.alwaysBuild || !exists) {
    // Rebuild index.js & WASM component.
    await spawn("npm", ["run", "build"], {
      stdio: "inherit",
    });

    // Deploy component to `<traildepot>/wasm`.
    await fs.mkdir(wasmPath, { recursive: true });
    await fs.copyFile(path.join("dist", COMPONENT_NAME), component);
  }

  info(`Component build & deploy took: ${(Date.now() - start) / 1000}s`);
}

async function hotRestart(opts: { pid: number; depotPath: string }) {
  // Rebuild and deploy component.
  await deployComponent({ depotPath: opts.depotPath, alwaysBuild: true });

  // SIGHUP `trail` process to re-load WASM component.
  kill(opts.pid, "SIGHUP");
}

function info(msg: string) {
  console.log(styleText("blue", msg));
}

async function fileExists(f: string): Promise<boolean> {
  try {
    await fs.stat(f);
    return true;
  } catch {
    return false;
  }
}
