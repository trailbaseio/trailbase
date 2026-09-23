import { initClient, Client } from "../src/index.ts";

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

export function envVarSet(name: string): boolean {
  const v = process.env[name];
  console.debug(`ENV[${name}]=${v}`);

  if (v === undefined) {
    return false;
  }

  if (v === "") {
    return true;
  }

  switch (v.toLowerCase()) {
    case "false":
    case "0":
      return false;
    default:
      return true;
  }
}

export async function connect(): Promise<Client> {
  const client = initClient(new URL(`http://${serverAddress()}`));
  await client.login("admin@localhost", "secret");
  return client;
}

const DEFAULT_PORT: number = 4005;
