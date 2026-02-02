import { createSignal, Switch, Match, createMemo } from "solid-js";
import { useQueryClient } from "@tanstack/solid-query";
import type { Row, ColumnDef } from "@tanstack/solid-table";

import { createConfigQuery, setConfig } from "@/lib/api/config";

import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardFooter,
  CardHeader,
} from "@/components/ui/card";
import { Table, buildTable } from "@/components/Table";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  DialogFooter,
} from "@/components/ui/dialog";
import { TextField, TextFieldInput } from "@/components/ui/text-field";

import { Config, DatabaseConfig } from "@proto/config";
import { TbLink, TbUnlink } from "solid-icons/tb";

export function DatabaseSettings(props: {
  markDirty: () => void;
  postSubmit: () => void;
}) {
  const config = createConfigQuery();

  return (
    <Switch>
      <Match when={config.isError}>Failed to fetch config</Match>

      <Match when={config.isLoading}>Loading</Match>

      <Match when={config.data?.config !== undefined}>
        <DatabaseSettingsForm config={config.data!.config!} {...props} />
      </Match>
    </Switch>
  );
}

function DatabaseSettingsForm(props: {
  config: Config;
  markDirty: () => void;
  postSubmit: () => void;
}) {
  const queryClient = useQueryClient();
  const [selectedRows, setSelectedRows] = createSignal(new Set<string>());
  const [linkDbDialog, setLinkDbDialog] = createSignal(false);

  const linkDb = async (name: string) => {
    const newConfig = Config.fromPartial(props.config);
    newConfig.databases = [...newConfig.databases, { name }];
    await setConfig(queryClient, newConfig, { throw: false });
  };

  const unlinkSelectedDbs = async () => {
    const newConfig = Config.fromPartial(props.config);

    const markedForUnlink = selectedRows();
    newConfig.databases = newConfig.databases.filter(
      (d) => !markedForUnlink.has(d.name ?? ""),
    );

    await setConfig(queryClient, newConfig, { throw: false });
  };

  const dbTable = createMemo(() => {
    return buildTable({
      columns: buildColumns(),
      data: props.config.databases,
      rowCount: props.config.databases.length,
      onRowSelection: (rows: Row<DatabaseConfig>[], value: boolean) => {
        const newSelection = new Set<string>(selectedRows());

        for (const row of rows) {
          const key = row.original.name;
          if (!key) {
            continue;
          }

          if (value) {
            newSelection.add(key);
          } else {
            newSelection.delete(key);
          }
        }
        setSelectedRows(newSelection);
      },
    });
  });

  let ref: HTMLInputElement | undefined;

  return (
    <Dialog
      id="link-db-dialog"
      open={linkDbDialog()}
      onOpenChange={setLinkDbDialog}
    >
      <>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Link Database</DialogTitle>
          </DialogHeader>

          <div class="flex w-full items-center gap-4">
            <span>Name: </span>

            <TextField class="grow">
              <TextFieldInput
                ref={ref}
                required={true}
                pattern="[a-zA-Z0-9_-]+"
                value={""}
                type="text"
              />
            </TextField>
          </div>

          <DialogFooter>
            <div class="flex w-full justify-between">
              <Button
                type="button"
                variant="outline"
                onClick={() => setLinkDbDialog(false)}
              >
                Cancel
              </Button>

              <Button
                type="button"
                onClick={() => {
                  const name = ref?.value;
                  if (name === undefined) {
                    return;
                  }

                  (async () => {
                    await linkDb(name);
                    setLinkDbDialog(false);
                    props.postSubmit();
                  })();
                }}
              >
                Link
              </Button>
            </div>
          </DialogFooter>
        </DialogContent>

        <div class="flex flex-col gap-4">
          <Card class="text-sm">
            <CardHeader>
              <h2>Linked Databases</h2>
            </CardHeader>

            <CardContent class="flex flex-col gap-4">
              <p>
                Additional databases can be linked and unlinked. For linked
                databases artifacts from{" "}
                <span class="font-mono">{"<traildepot>/data/<name>.db"}</span>{" "}
                and{" "}
                <span class="font-mono">
                  {"<traildepot>/migrations/<name>/"}
                </span>{" "}
                will be picked up. Unlinking a databases does not clean up any
                artifacts.
              </p>

              <p>
                Databases are an isolation boundary. They can be accessed
                independently w/o locking, which also implies that{" "}
                <span class="font-mono">FOREIGN KEY</span>s and{" "}
                <span class="font-mono">TRIGGER</span>s cannot cross this
                boundary. For most use-cases it's probably best to start not
                linking additional databases and add more only when physical
                isolation is warranted.
              </p>

              <div class="max-h-[500px] overflow-auto">
                <Table table={dbTable()} loading={false} />
              </div>
            </CardContent>

            <CardFooter>
              <div class="flex w-full justify-between gap-2">
                <DialogTrigger>
                  <Button variant="outline" type="button">
                    <TbLink />
                    Link
                  </Button>
                </DialogTrigger>

                <Button
                  variant="destructive"
                  type="button"
                  disabled={selectedRows().size === 0}
                  onClick={() => {
                    (async () => {
                      await unlinkSelectedDbs();

                      props.postSubmit();
                    })();
                  }}
                >
                  <TbUnlink />
                  Unlink
                </Button>
              </div>
            </CardFooter>
          </Card>

          <ImportExportCard />
        </div>
      </>
    </Dialog>
  );
}

function ImportExportCard() {
  return (
    <Card class="text-sm">
      <CardHeader>
        <h2>Data Import {"&"} Export</h2>
      </CardHeader>

      <CardContent>
        <p class="mt-2">
          Importing and exporting data via the UI is not yet supported. Instead,
          you can use the <span class="font-mono">sqlite3</span> command line
          interface. TrailBase does not require any special metadata. Any{" "}
          <span class="font-mono">STRICT</span>ly typed{" "}
          <span class="font-mono">TABLE</span> with an
          <span class="font-mono">INTEGER</span> or UUID primary key can be
          exposed via TrailBase's Record APIs.
        </p>

        <p class="my-2">Import, e.g.:</p>
        <pre class="ml-4 whitespace-pre-wrap">
          $ cat import_data.sql | sqlite3 traildepot/data/main.db
        </pre>

        <p class="my-2">Export, e.g.:</p>

        <pre class="ml-4 whitespace-pre-wrap">
          $ sqlite3 traildepot/data/main.db
          <br />
          sqlite&gt; .output dump.db
          <br />
          sqlite&gt; .dump
          <br />
        </pre>
      </CardContent>
    </Card>
  );
}

function buildColumns(): ColumnDef<DatabaseConfig>[] {
  return [
    {
      header: "name",
      accessorKey: "name",
      cell: (ctx) => {
        const name = ctx.row.original.name;
        return <div class="min-w-[200px]">{name ?? "<missing>"}</div>;
      },
    },
  ];
}
