/* @refresh reload */
import './index.css'
import { hydrate } from 'solid-js/web'
import { App, type InitialData } from './App'

hydrate(
  () => {
    const initialData: InitialData = window.__INITIAL_DATA__!;
    return (
      <App {...initialData} />
    );
  },
  document.getElementById('root') as HTMLElement,
);
