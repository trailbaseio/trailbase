import { QueryClient } from "@tanstack/query-core";
import {
  useLiveQuery,
  useOptimisticMutation,
  createCollection,
} from "@tanstack/react-db";
import { queryCollectionOptions } from "@tanstack/db-collections";

import { Client } from "trailbase";
import "./App.css";
import type { FormEvent } from "react";

const client = Client.init("http://localhost:4000");

type Data = {
  id: number | null;
  updated: number | null;
  data: string;
};

const queryClient = new QueryClient();

const dataCollection = createCollection(
  queryCollectionOptions<Data>({
    id: "data",
    queryKey: ["data"],
    queryFn: async () => (await client.records("data").list<Data>()).records,
    getKey: (item) => item.id?.toString() ?? "??",
    queryClient: queryClient,
  }),
);

function App() {
  const { data } = useLiveQuery((q) =>
    q
      .from({ dataCollection })
      .orderBy(`@updated`)
      .select(`@id`, `@updated`, `@data`),
  );

  // Define mutations
  const addData = useOptimisticMutation({
    mutationFn: async ({ transaction }) => {
      const { changes: newData } = transaction.mutations[0];
      const response = await client.records("data").create(newData as Data);
      const _ = response;

      await dataCollection.utils.refetch();
      // TODO: We should await the update.
    },
  });

  function handleSubmit(e: FormEvent) {
    // Don't reload the page.
    e.preventDefault();

    const form = e.target;
    const formData = new FormData(form as HTMLFormElement);

    const formJson = Object.fromEntries(formData.entries());
    addData.mutate(() => {
      dataCollection.insert({
        id: null,
        updated: null,
        data: formJson.text as string,
      });
    });

    console.log(formJson);
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
            {data.map((d) => (
              <tr>
                <td>{d.id}</td>
                <td>{d.updated}</td>
                <td>{d.data}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <p className="read-the-docs">
        <form method="post" onSubmit={handleSubmit}>
          <input name="text" type="text" />

          <button type="submit">submit</button>
        </form>
      </p>
    </>
  );
}

export default App;
