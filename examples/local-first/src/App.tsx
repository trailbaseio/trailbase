import { useState, type FormEvent } from "react";
import { QueryClient } from "@tanstack/query-core";
import { useLiveQuery, createCollection } from "@tanstack/react-db";
import { queryCollectionOptions } from "@tanstack/query-db-collection";
import { trailBaseCollectionOptions } from "@tanstack/trailbase-db-collection";
import { initClient, type Client } from "trailbase";

import { getComplementaryColor } from "./lib/color";

const client: Client = initClient("http://localhost:4000");
const queryClient = new QueryClient();
const useTrailBase = true;

type Config = {
  id: number;
  key: string;
  value: string;
  created_at: number;
  updated_at: number;
};

const configCollection = useTrailBase
  ? createCollection(
      trailBaseCollectionOptions<Config>({
        recordApi: client.records<Config>("config"),
        getKey: (item) => item.id ?? -1,
        parse: {},
        serialize: {},
      }),
    )
  : createCollection(
      queryCollectionOptions<Config>({
        id: "config",
        queryKey: ["config"],
        queryFn: async () => {
          const data = client.records<Config>("config");
          return (await data.list()).records;
        },
        getKey: (item) => item.id ?? -1,
        queryClient: queryClient,
      }),
    );

type Todo = {
  id: number;
  text: string;
  completed: boolean;
  created_at: number;
  updated_at: number;
};

const todoCollection = useTrailBase
  ? createCollection(
      trailBaseCollectionOptions<Todo>({
        recordApi: client.records<Todo>("todos"),
        getKey: (item) => item.id ?? -1,
        parse: {},
        serialize: {},
      }),
    )
  : createCollection(
      queryCollectionOptions<Todo>({
        id: "todos",
        queryKey: ["todos"],
        queryFn: async () => {
          const data = client.records<Todo>("todos");
          return (await data.list()).records;
        },
        getKey: (item) => item.id ?? -1,
        queryClient: queryClient,
      }),
    );

function now(): number {
  return Math.floor(Date.now() / 1000);
}

function App() {
  // Get data using live queries with TrailBase collections
  const { data: todos } = useLiveQuery((q) =>
    q
      .from({ todo: todoCollection })
      .orderBy(({ todo }) => todo.created_at, `asc`),
  );

  const { data: configData } = useLiveQuery((q) =>
    q.from({ config: configCollection }),
  );

  const [newTodo, setNewTodo] = useState(``);

  // Define a type-safe helper function to get config values
  const getConfigValue = (key: string): string | undefined => {
    for (const config of configData) {
      if (config.key === key) {
        return config.value;
      }
    }
    return undefined;
  };

  // Define a helper function to update config values
  const setConfigValue = (key: string, value: string): void => {
    for (const config of configData) {
      if (config.key === key) {
        configCollection.update(config.id, (draft) => {
          draft.value = value;
        });
        return;
      }
    }

    // If the config doesn't exist yet, create it
    configCollection.insert({
      id: Math.round(Math.random() * 1000000),
      key,
      value,
      created_at: now(),
      updated_at: now(),
    });
  };

  const backgroundColor = getConfigValue(`backgroundColor`);
  const titleColor = getComplementaryColor(backgroundColor);

  const handleColorChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const newColor = e.target.value;
    setConfigValue(`backgroundColor`, newColor);
  };

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    const todo = newTodo.trim();
    setNewTodo(``);

    if (todo) {
      todoCollection.insert({
        text: todo,
        completed: false,
        id: Math.round(Math.random() * 1000000),
        created_at: now(),
        updated_at: now(),
      });
    }
  };

  const activeTodos = todos.filter((todo) => !todo.completed);
  const completedTodos = todos.filter((todo) => todo.completed);

  return (
    <main
      className="flex h-dvh justify-center overflow-auto py-8"
      style={{ backgroundColor }}
    >
      <div className="w-[550px]">
        <h1
          className="mb-4 text-center text-[70px] font-bold"
          style={{ color: titleColor }}
        >
          TrailBase Todo
        </h1>

        <div className="flex justify-end py-4">
          <div className="flex items-center">
            <label
              htmlFor="colorPicker"
              className="mr-2 text-sm font-medium text-gray-700"
              style={{ color: titleColor }}
            >
              Background Color:
            </label>
            <input
              type="color"
              id="colorPicker"
              value={backgroundColor}
              onChange={handleColorChange}
              className="cursor-pointer rounded border border-gray-300"
            />
          </div>
        </div>

        <div className="relative bg-white shadow-[0_2px_4px_0_rgba(0,0,0,0.2),0_25px_50px_0_rgba(0,0,0,0.1)]">
          <form onSubmit={handleSubmit} className="relative">
            <button
              type="button"
              className="absolute h-full w-12 text-[30px] text-[#e6e6e6] hover:text-[#4d4d4d]"
              disabled={todos.length === 0}
              onClick={() => {
                const todosToToggle =
                  activeTodos.length > 0 ? activeTodos : completedTodos;

                todoCollection.update(
                  todosToToggle.map((todo) => todo.id),
                  (drafts) =>
                    drafts.forEach(
                      (draft) => (draft.completed = !draft.completed),
                    ),
                );
              }}
            >
              ❯
            </button>
            <input
              type="text"
              value={newTodo}
              onChange={(e) => setNewTodo(e.target.value)}
              placeholder="What needs to be done?"
              className="box-border h-[64px] w-full border-none pr-4 pl-[60px] text-2xl font-light shadow-[inset_0_-2px_1px_rgba(0,0,0,0.03)]"
            />
          </form>

          <ul className="list-none">
            {todos.map((todo) => (
              <li
                key={`todo-${todo.id}`}
                className="group relative border-b border-[#ededed] last:border-none"
              >
                <div className="gap-1.2 flex h-[58px] items-center pl-[60px]">
                  <input
                    type="checkbox"
                    checked={todo.completed}
                    onChange={() =>
                      todoCollection.update(todo.id, (draft) => {
                        draft.completed = !draft.completed;
                      })
                    }
                    className="absolute left-[12px] size-[40px] cursor-pointer"
                  />
                  <label
                    className={`block p-[15px] text-2xl transition-colors ${todo.completed ? `text-[#d9d9d9] line-through` : ``}`}
                  >
                    {todo.text}
                  </label>
                  <button
                    onClick={() => todoCollection.delete(todo.id)}
                    className="absolute right-[20px] hidden text-[30px] text-[#cc9a9a] transition-colors group-hover:block hover:text-[#af5b5e]"
                  >
                    ×
                  </button>
                </div>
              </li>
            ))}
          </ul>

          <footer className="flex h-[40px] items-center justify-between border-t border-[#e6e6e6] px-[15px] text-[14px] text-[#777]">
            <span>
              {`${activeTodos.length} ${activeTodos.length === 1 ? `item` : `items`} left`}
            </span>

            {completedTodos.length > 0 && (
              <button
                onClick={() =>
                  todoCollection.delete(completedTodos.map((todo) => todo.id))
                }
                className="hover:underline"
              >
                Clear completed
              </button>
            )}
          </footer>
        </div>
      </div>
    </main>
  );
}

export default App;
