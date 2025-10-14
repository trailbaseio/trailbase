# Example WASM Component

An example TypeScript WASM component for TrailBase. Feel free to copy and make it your own.

## Running in Development

Assuming you have `trail` [installed](https://trailbase.io/getting-started/install) and in your `$PATH`, simply run

```sh
npm run dev
```

to start a TrailBase server with the component and a file watcher.

Whenever a file under `./src` changes, the watcher will rebuild the component and send a `SIGHUP` ot the server to reload the component. This will typically take a few seconds.

### Limitations

Adding or removing new routes will require restarting the server.
