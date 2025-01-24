import { renderToString, generateHydrationScript } from 'solid-js/web'
import { App, type Clicked } from './App'

export function render(_url: string, count: number) {
  const data = { count } satisfies Clicked;

  return {
    head: generateHydrationScript(),
    html: renderToString(() => <App initialCount={count} />),
    data: `<script>window.__INITIAL_DATA__ = ${JSON.stringify(data)};</script>`,
  };
}
