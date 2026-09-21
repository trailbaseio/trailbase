import { renderToString, generateHydrationScript } from 'solid-js/web'
import { App, type InitialData } from './App'

export function render(_url: string, clickCount: number, error?: string) {
  const data = { initialClickCount: clickCount, error } satisfies InitialData;

  return {
    head: generateHydrationScript(),
    html: renderToString(() => <App {...data} />),
    data: `<script>window.__INITIAL_DATA__ = ${JSON.stringify(data)};</script>`,
  };
}
