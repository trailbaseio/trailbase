import { defineConfig } from 'vite';

import tsconfigPaths from 'vite-tsconfig-paths';

export default defineConfig({
  base: "/_/admin",
  plugins: [
    tsconfigPaths(),
  ],
  server: {
    port: 3000,
  },
  build: {
    target: 'esnext',
  },
});
