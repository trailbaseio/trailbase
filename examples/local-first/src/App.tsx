import { QueryClient } from "@tanstack/query-core";
import {
  useLiveQuery,
  createCollection,
  type MutationFn,
} from "@tanstack/react-db";
import { queryCollectionOptions } from "@tanstack/db-collections";

import { Client } from "trailbase";
import { useState } from "react";
import type { FormEvent } from "react";

import { trailBaseCollectionOptions } from "./lib/trailbase.ts";
import "./App.css";

const client = Client.init("http://localhost:4000");

type Data = {
  id: number | null;
  updated: number | null;
  data: string;
};

const queryClient = new QueryClient();
const useTrailBase = true;

function buildMutationFn<T extends object>(client: Client): MutationFn<T> {
  return async ({ transaction }) => {
    for (const tx of transaction.mutations) {
      const { key, type, changes } = tx;

      if (type === "insert") {
        // NOTE: We could also do bulk inserts.
        await client.records("data").create(changes as Data);
      } else if (type === "update") {
        await client.records("data").update(key, changes as Data);
      } else if (type === "delete") {
        await client.records("data").delete(key);
      }
    }

    if (!useTrailBase) {
      await dataCollection.utils.refetch();
    }
  };
}

const dataCollection = createCollection(
  useTrailBase
    ? trailBaseCollectionOptions<Data>({
        client,
        recordApi: "data",
        getKey: (item) => item.id ?? -1,
      })
    : queryCollectionOptions<Data>({
        id: "data",
        queryKey: ["data"],
        queryFn: async () =>
          (await client.records("data").list<Data>()).records,
        getKey: (item) => item.id ?? -1,
        queryClient: queryClient,
      }),
);

function App() {
  const [input, setInput] = useState("");

  const { data } = useLiveQuery((q) =>
    q
      .from({ dataCollection })
      .orderBy(`@updated`)
      .select(`@id`, `@updated`, `@data`),
  );

  function handleSubmit(e: FormEvent) {
    e.preventDefault(); // Don't reload the page.

    const form = e.target;
    const formData = new FormData(form as HTMLFormElement);

    const formJson = Object.fromEntries(formData.entries());
    const text = formJson.text as string;

    if (text) {
      dataCollection.insert({
        id: null,
        updated: null,
        data: formJson.text as string,
      });
    }
  }

  return (
    <>
      <h1>Local First</h1>

      <div className="card">
        <table>
          <thead>
            <tr>
              <th>id</th>
              <th>updated</th>
              <th>data</th>
            </tr>
          </thead>

          <tbody>
            {data.map((d, idx) => (
              <tr key={`row-${idx}`}>
                <td>{d.id}</td>
                <td>{d.updated}</td>
                <td>{d.data}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <form method="post" onSubmit={handleSubmit}>
        <p className="read-the-docs">
          <input
            name="text"
            type="text"
            onInput={(e) => setInput(e.currentTarget.value)}
          />

          <button type="submit" disabled={input === ""}>
            submit
          </button>
        </p>
      </form>
    </>
  );
}

export default App;
