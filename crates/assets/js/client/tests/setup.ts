import { initClient, Client } from "../src/index";

export function serverPort(): number {
  const env = process.env["PORT"];
  if (env) {
    return parseInt(env);
  }
  return DEFAULT_PORT;
}

export function serverAddress(): string {
  return `127.0.0.1:${serverPort()}`;
}

export function useWebSocket(): boolean {
  const env = process.env["USE_WS"];
  switch (env?.toUpperCase()) {
    case "TRUE":
    case "1":
      return true;
    default:
      return false;
  }
}

export async function connect(): Promise<Client> {
  const client = initClient(new URL(`http://${serverAddress()}`));
  await client.login("admin@localhost", "secret");
  return client;
}

const DEFAULT_PORT: number = 4005;
