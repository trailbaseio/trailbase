export const jsonContentTypeHeader = {
  "Content-Type": "application/json",
};

function _isDev(): boolean {
  type ImportMeta = {
    env: object | undefined;
  };
  const env = (import.meta as unknown as ImportMeta).env;
  const key = "DEV" as keyof typeof env;
  const isDev = env?.[key] ?? false;

  return isDev;
}

export const isDev = _isDev();
