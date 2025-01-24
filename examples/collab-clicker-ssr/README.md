# Collaborative Clicker App with SSR

This app serves as a showcase for "realtime" API subscriptions and server-side
rendering (SSR). This app uses SolidJS but the same approach applies to React,
Vue, Svelte, Preact, ... .

## Generate the App

```bash
pnpm create vite@latest <project-dir>
```

Follow CLI template assistant: `Others > create-vite-extra > ssr-[vue|react|preact|svelte|solid]`.

The resulting template is set up to use "express.js", see
'<project-dir>/server.js'. In order to serve with TrailBase instead we have to:

1. Tell vite to inline deps when building the server entry point:
    ```bash
    cat <project-dir>/vite.config.ts

    export default defineConfig({
      plugins: [<react|vue|solid|svelte|...>],
      ssr: {
        noExternal: true,  // IMPORTANT
      },
    })
    ```
2. Set up `traildepot/scripts/main.ts` to:
    * pick up the generated HTML template for the client,
    * generate the hydration script in the HTML head,
    * and run the server's entry-point to render the initial HTML body.
