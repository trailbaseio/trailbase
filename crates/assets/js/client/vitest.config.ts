import { defineConfig } from "vitest/config";
import type { TestTagDefinition } from "vitest/config";
import { envVarSet } from "./tests/util.ts";

const isCi = envVarSet("CI");
const useWebSocket = envVarSet("USE_WS");

const integrationTestTag: TestTagDefinition = {
  name: "integration",
} as const;

export default defineConfig({
  test: {
    // No fancy terminal sequences, append everything in order.
    reporters: [isCi ? "tap" : "verbose"],
    projects: [
      {
        resolve: {
          tsconfigPaths: true,
        },
        test: {
          name: "unit-tests",
          globals: true,
          fileParallelism: true,
          environment: "jsdom",
          include: ["tests/unit/**/*.test.ts", "tests/unit/**/*.bench.ts"],
        },
      },
      {
        test: {
          name: "integration-tests running in jsdom ('browser')",
          tags: [integrationTestTag],
          // NOTE: We cannot use `jsdom` due to it having a colliding `Event`
          // definition breaking `undici`, which then breaks our WebSocket
          // tests :/
          //   https://github.com/nodejs/undici/issues/2663#issuecomment-1936036650
          environment: useWebSocket ? "node" : "jsdom",
          include: ["tests/integration/**/*.test.ts"],
          globalSetup: ["tests/start_server.ts", "tests/start_oauth_server.ts"],
          fileParallelism: isCi ? true : false,
        },
      },
    ],
  },
});
