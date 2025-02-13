import {
  For,
  Match,
  Show,
  Switch,
  createEffect,
  createResource,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import { createWritableMemo } from "@solid-primitives/memo";
import type { Accessor, Signal } from "solid-js";
import type { ColumnDef } from "@tanstack/solid-table";
import { persistentAtom } from "@nanostores/persistent";
import { useStore } from "@nanostores/solid";
import { TbTrash, TbEdit, TbDeviceFloppy, TbHelp } from "solid-icons/tb";

import { Separator } from "@/components/ui/separator";
import { EditorView, keymap, lineNumbers, gutter } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { defaultKeymap } from "@codemirror/commands";
import {
  syntaxHighlighting,
  defaultHighlightStyle,
} from "@codemirror/language";
import { sql } from "@codemirror/lang-sql";

import { SplitView } from "@/components/SplitView";
import {
  Resizable,
  ResizablePanel,
  ResizableHandle,
} from "@/components/ui/resizable";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  DialogFooter,
} from "@/components/ui/dialog";
import { TextField, TextFieldInput } from "@/components/ui/text-field";

import { DataTable } from "@/components/Table";
import type { QueryRequest, QueryResponse } from "@/lib/bindings";
import { adminFetch } from "@/lib/fetch";
import { isNotNull } from "@/lib/schema";

async function executeSql(
  sql: string | undefined,
): Promise<QueryResponse | undefined> {
  if (sql === undefined) {
    return undefined;
  }

  const response = await adminFetch("/query", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      query: sql,
    } as QueryRequest),
  });

  return await response.json();
}

type RowData = Array<object>;

function View(props: { response: Accessor<QueryResponse | undefined> }) {
  const response = () => props.response();

  const columnDefs = (): ColumnDef<RowData>[] => {
    return (response()?.columns ?? []).map((col, idx) => {
      const notNull = isNotNull(col.options);

      const header = `${col.name} [${col.data_type}${notNull ? "" : "?"}]`;
      return {
        accessorFn: (row) => row[idx],
        header,
      };
    });
  };

  return (
    <Show when={response()} fallback={<>No Data</>}>
      <Switch>
        <Match when={(response()?.columns?.length ?? 0) > 0}>
          <DataTable
            columns={columnDefs}
            data={() => response()!.rows as RowData[]}
          />
        </Match>

        <Match when={(response()?.columns?.length ?? 0) == 0}>
          No data returned by query
        </Match>
      </Switch>
    </Show>
  );
}

function SideBar(props: {
  selectedSignal: Signal<number>;
  horizontal: boolean;
}) {
  const [selected, setSelected] = props.selectedSignal;
  const scripts = useStore($scripts);

  function addNewScript() {
    const s = [
      ...$scripts.get(),
      {
        name: "New Script",
        contents: "SELECT COUNT(*) FROM _user;",
      },
    ];
    $scripts.set(s);

    setSelected(s.length - 1);
  }

  const flexStyle = () => (props.horizontal ? "flex flex-col h-dvh" : "flex");
  return (
    <div class={`${flexStyle()} m-4 gap-2`}>
      <For each={scripts()}>
        {(_script: Script, index: Accessor<number>) => {
          const scriptName = () => scripts()[index()].name;
          return (
            <Button
              variant={selected() === index() ? "default" : "outline"}
              onClick={() => {
                setSelected(index());
              }}
            >
              {scriptName()}
            </Button>
          );
        }}
      </For>

      <Button variant="secondary" onClick={addNewScript}>
        New
      </Button>
    </div>
  );
}

function HelpDialog() {
  return (
    <Dialog id="edit-help">
      <DialogTrigger>
        <TbHelp />
      </DialogTrigger>

      <DialogContent>
        <DialogHeader>
          <DialogTitle>Editor Help</DialogTitle>
        </DialogHeader>

        <p>
          The editor lets you execute arbitrary SQL statements, so be careful
          with what you wish for. If you just want to experiment, consider
          working on a non-prod data set or a copy.
        </p>

        <p>
          Further note that there's no pagination, so whatever you query will be
          returned. Working on large data sets, you might want to{" "}
          <span class="text-mono font-mono">LIMIT</span> your result size.
        </p>

        <p>
          Also note that scripts are currently stored in your browser's local
          storage. This means, switching devices, browsers, or the origin of
          your website, you won't have access to your scripts. This is something
          we'd like to lower into the database layer in the future.
        </p>
      </DialogContent>
    </Dialog>
  );
}

function RenameDialog(props: { selected: number; script: Script }) {
  const [open, setOpen] = createSignal(false);
  const [name, setName] = createWritableMemo(() => props.script.name);

  const onSubmit = () => {
    updateExistingScript(props.selected, {
      ...props.script,
      name: name(),
    });
    setOpen(false);
  };

  return (
    <Dialog id="rename" open={open()} onOpenChange={setOpen}>
      <DialogTrigger>
        <TbEdit />
      </DialogTrigger>

      <DialogContent>
        <DialogHeader>
          <DialogTitle>Rename</DialogTitle>
        </DialogHeader>

        <form
          class="flex flex-col gap-4 py-12 px-8"
          onSubmit={onSubmit}
          action="javascript:void(0);"
        >
          <TextField>
            <TextFieldInput
              required
              value={name()}
              type="text"
              onKeyUp={(e: Event) => {
                setName((e.target as HTMLInputElement).value);
              }}
            />
          </TextField>

          <DialogFooter>
            <Button type="submit">Save</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function EditorPanel(props: { selectedSignal: Signal<number> }) {
  const [selected, setSelected] = props.selectedSignal;

  const scripts = useStore($scripts);
  const script = (): Script => {
    const s = scripts();
    if (selected() < s.length) {
      return s[selected()];
    }
    if (s.length === 0) {
      return defaultScript;
    }
    return s[s.length - 1];
  };

  const [queryString, setQueryString] = createSignal<string | undefined>();
  const [executionResult, { mutate, refetch }] = createResource(
    queryString,
    executeSql,
  );

  createEffect(() => {
    // Subscribe to selected script changes and reset the query results.
    selected();
    const r = executionResult();
    if (r && editor?.state.doc.toString() !== queryString()) {
      mutate(undefined);
    }
  });

  const execute = () => {
    const text = editor?.state.doc.toString();
    if (text) {
      // We need to distinguish to work-around createResources caching.
      if (queryString() === text) {
        refetch();
      } else {
        setQueryString(text);
      }
    }
  };

  let ref: HTMLDivElement | undefined;
  let editor: EditorView | undefined;

  const newEditorState = (contents: string) =>
    EditorState.create({
      doc: contents,
      extensions: [
        myTheme,
        keymap.of([
          {
            key: "Ctrl-Enter",
            run: () => {
              execute();
              return true;
            },
          },
          ...defaultKeymap,
        ]),
        lineNumbers(),
        gutter({ class: "cm-mygutter" }),
        sql(),
        syntaxHighlighting(defaultHighlightStyle),
      ],
    });

  onCleanup(() => {
    console.debug("editor cleanup");
    editor?.destroy();
  });
  onMount(() => {
    editor?.destroy();
    editor = new EditorView({
      state: newEditorState(script().contents),
      parent: ref!,
    });
    editor.focus();
  });

  createEffect(() => {
    console.debug("setting editor state");
    const s = script();
    editor?.setState(newEditorState(s.contents));
  });

  return (
    <>
      <Resizable orientation="vertical" class="h-full">
        <ResizablePanel>
          <h1 class="m-4 flex justify-between items-center gap-4">
            <div class="flex items-center gap-4">
              <span class="text-accent-600">Editor</span>
              <span class="text-accent-600">&gt;</span>
              <span>{script().name}</span>

              <RenameDialog selected={selected()} script={script()} />

              <TbDeviceFloppy
                onClick={() => {
                  const e = editor;
                  if (e) {
                    updateExistingScript(selected(), {
                      ...script(),
                      contents: e.state.doc.toString(),
                    });
                  }
                }}
              />
            </div>

            <div class="flex gap-4">
              <HelpDialog />
              <TbTrash
                onClick={() => {
                  $scripts.set($scripts.get().toSpliced(selected(), 1));
                  setSelected(Math.max(0, selected() - 1));
                }}
              />
            </div>
          </h1>

          <Separator />

          <div class="p-4 flex flex-col justify-between gap-4 overflow-y-auto">
            {/* Editor */}
            <div class="overflow-y-scroll outline outline-1 rounded">
              <div ref={ref} />
            </div>

            <div class="flex justify-end">
              <Button variant="destructive" onClick={execute}>
                Execute (Ctrl+Enter)
              </Button>
            </div>
          </div>
        </ResizablePanel>

        <ResizableHandle withHandle={true} />

        <ResizablePanel class="grow overflow-y-scroll hide-scrollbars p-4">
          <View response={executionResult} />
        </ResizablePanel>
      </Resizable>
    </>
  );
}

export function EditorPage() {
  const selectedSignal = createSignal<number>(0);

  return (
    <SplitView
      first={(props: { horizontal: boolean }) => {
        return (
          <SideBar
            selectedSignal={selectedSignal}
            horizontal={props.horizontal}
          />
        );
      }}
      second={() => {
        return <EditorPanel selectedSignal={selectedSignal} />;
      }}
    />
  );
}

export default EditorPage;

const myTheme = EditorView.theme(
  {
    ".cm-gutters": {
      backgroundColor: "#eeeeee",
      color: "#000",
      border: "none",
    },
  },
  { dark: false },
);

type Script = {
  name: string;
  contents: string;
};

const defaultScript: Script = {
  name: "Select",
  contents: "SELECT\n  *\nFROM\n  _user;",
};

// NOTE: It seems like "nanostores" diffs array contents. It re-renders, if the array
// object is different and at least one of the contained objects has a different id.
// In other words just copying the array and setting a new Script.name, doesn't trigger,
// we have to replace the entire script.
// If this behavior is documented somewhere, I couldn't find it. I wish it would be less
// smart :/.
function updateExistingScript(index: number, script: Script) {
  const s = [...$scripts.get()];
  s[index] = {
    ...script,
  };
  $scripts.set(s);
}

const $scripts = persistentAtom<Script[]>("scripts", [defaultScript], {
  encode: JSON.stringify,
  decode: JSON.parse,
});
