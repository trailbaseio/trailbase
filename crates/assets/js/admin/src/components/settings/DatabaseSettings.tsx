import {
  createSignal,
  Switch,
  Match,
  For,
  Show,
  onCleanup,
  createEffect,
} from "solid-js";
import { useQueryClient } from "@tanstack/solid-query";
import { TbOutlineLink, TbOutlineUnlink } from "solid-icons/tb";

import { createConfigQuery, setConfig } from "@/lib/api/config";

import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardFooter,
  CardHeader,
} from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Checkbox } from "@/components/ui/checkbox";
import { TextField, TextFieldInput } from "@/components/ui/text-field";

import { Config, DatabaseConfig } from "@proto/config";
import { createSystemInfoQuery } from "@/lib/api/info";

export function DatabaseSettings(props: {
  setDirty: (dirty: boolean) => void;
  postSubmit: () => void;
}) {
  const config = createConfigQuery();
  const systemInfo = createSystemInfoQuery();
  const isPostgres = () => systemInfo.data?.postgres;

  return (
    <Switch>
      <Match when={systemInfo.isLoading}>
        <p role="status">Loading database settings...</p>
      </Match>
      <Match when={systemInfo.isError}>
        <p role="alert">Unable to load system information. Try again.</p>
      </Match>
      <Match when={isPostgres() === true}>
        <div class="flex flex-col gap-4">
          <Card class="text-sm">
            <CardHeader>
              <h2>Linked Databases</h2>
            </CardHeader>

            <CardContent class="flex flex-col gap-4">
              Not supported in Postgres mode.
            </CardContent>
          </Card>
        </div>
      </Match>
      <Match when={config.isLoading}>
        <p role="status">Loading database settings...</p>
      </Match>
      <Match when={config.isError}>
        <p role="alert">Unable to load configuration. Try again.</p>
      </Match>

      <Match when={config.data?.config !== undefined}>
        <DatabaseSettingsForm config={config.data!.config!} {...props} />
      </Match>
    </Switch>
  );
}

const databaseNamePattern = /^[A-Za-z0-9_-]+$/;
const reservedDatabaseNames = new Set(["main", "public", "logs", "session"]);

function cloneConfig(config: Config) {
  return Config.decode(Config.encode(config).finish());
}

export function validateDatabaseName(name: string, existing: DatabaseConfig[]) {
  const trimmed = name.trim();
  if (!trimmed) return "Enter a database name.";
  if (!databaseNamePattern.test(trimmed))
    return "Use only letters, numbers, underscores, and hyphens.";
  if (reservedDatabaseNames.has(trimmed))
    return "That database name is reserved.";
  if (existing.some((database) => database.name === trimmed))
    return "That database is already linked.";
  return undefined;
}

function DatabaseSettingsForm(props: {
  config: Config;
  setDirty: (dirty: boolean) => void;
  postSubmit: () => void;
}) {
  const queryClient = useQueryClient();
  const [selectedRows, setSelectedRows] = createSignal(new Set<string>());
  const [linkOpen, setLinkOpen] = createSignal(false);
  const [unlinkOpen, setUnlinkOpen] = createSignal(false);
  const [name, setName] = createSignal("");
  const [pending, setPending] = createSignal(false);
  const [error, setError] = createSignal<string>();
  let active = true;
  onCleanup(() => {
    active = false;
  });
  createEffect(() => props.setDirty(false));
  const validation = () => validateDatabaseName(name(), props.config.databases);
  const selected = () =>
    props.config.databases.filter((db) => selectedRows().has(db.name ?? ""));
  const namedDatabases = () =>
    props.config.databases.filter((db) => db.name !== undefined);
  createEffect(() => {
    const names = new Set(namedDatabases().map((db) => db.name!));
    const retained = new Set(
      [...selectedRows()].filter((selectedName) => names.has(selectedName)),
    );
    if (retained.size !== selectedRows().size) setSelectedRows(retained);
  });
  const allSelected = () =>
    namedDatabases().length > 0 &&
    selected().length === namedDatabases().length;

  const link = async () => {
    const message = validation();
    if (message || pending()) return setError(message);
    setPending(true);
    setError();
    try {
      const config = cloneConfig(props.config);
      config.databases = [...config.databases, { name: name().trim() }];
      await setConfig({ client: queryClient, config, throw: true });
      if (!active) return;
      setLinkOpen(false);
      setName("");
      props.postSubmit();
    } catch {
      if (active) setError("Unable to link database. Try again.");
    } finally {
      if (active) setPending(false);
    }
  };
  const unlink = async () => {
    if (pending() || selected().length === 0) return;
    setPending(true);
    setError();
    try {
      const config = cloneConfig(props.config);
      const remove = new Set(selected().map((db) => db.name));
      config.databases = config.databases.filter((db) => !remove.has(db.name));
      await setConfig({ client: queryClient, config, throw: true });
      if (!active) return;
      setUnlinkOpen(false);
      setSelectedRows(new Set<string>());
      props.postSubmit();
    } catch {
      if (active) setError("Unable to unlink databases. Try again.");
    } finally {
      if (active) setPending(false);
    }
  };
  return (
    <>
      <Dialog
        open={linkOpen()}
        onOpenChange={(open) => !pending() && setLinkOpen(open)}
      >
        <DialogContent closeDisabled={pending()}>
          <DialogHeader>
            <DialogTitle>Link Database</DialogTitle>
          </DialogHeader>
          <TextField
            class="grow"
            validationState={validation() ? "invalid" : "valid"}
          >
            <label for="database-name">Name</label>
            <TextFieldInput
              id="database-name"
              type="text"
              value={name()}
              required
              pattern="[A-Za-z0-9_-]+"
              onInput={(e) => setName(e.currentTarget.value)}
              aria-describedby="database-name-help"
            />
            <p id="database-name-help">
              Letters, numbers, underscores, and hyphens only. Names main,
              public, logs, and session are reserved.
            </p>
            <Show when={validation()}>
              <p role="alert">{validation()}</p>
            </Show>
          </TextField>
          <Show when={error()}>
            <p role="alert">{error()}</p>
          </Show>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              disabled={pending()}
              onClick={() => setLinkOpen(false)}
            >
              Cancel
            </Button>
            <Button
              type="button"
              disabled={!!validation() || pending()}
              onClick={() => void link()}
            >
              {pending() ? "Linking…" : "Link"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog
        open={unlinkOpen()}
        onOpenChange={(open) => !pending() && setUnlinkOpen(open)}
      >
        <DialogContent closeDisabled={pending()}>
          <DialogHeader>
            <DialogTitle>Unlink databases</DialogTitle>
          </DialogHeader>
          <p>
            Unlink{" "}
            {selected()
              .map((db) => db.name)
              .join(", ")}
            ?
          </p>
          <Show when={error()}>
            <p role="alert">{error()}</p>
          </Show>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              disabled={pending()}
              onClick={() => setUnlinkOpen(false)}
            >
              Cancel
            </Button>
            <Button
              type="button"
              variant="destructive"
              disabled={pending()}
              onClick={() => void unlink()}
            >
              {pending() ? "Unlinking…" : "Confirm"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <div class="flex flex-col gap-4">
        <Card class="text-sm">
          <CardHeader>
            <h2>Linked Databases</h2>
          </CardHeader>
          <CardContent class="flex flex-col gap-4">
            <p>
              Additional databases can be linked and unlinked. For linked
              databases artifacts from{" "}
              <span class="font-mono">{"<traildepot>/data/<name>.db"}</span> and{" "}
              <span class="font-mono">{"<traildepot>/migrations/<name>/"}</span>{" "}
              will be picked up. Unlinking a database does not clean up
              artifacts.
            </p>
            <p>
              Databases are an isolation boundary; foreign keys and triggers
              cannot cross this boundary.
            </p>
            <div class="max-h-[500px] overflow-auto">
              <div class="overflow-x-auto">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead class="w-10">
                        <Checkbox
                          aria-label="Select all databases"
                          checked={allSelected()}
                          onChange={(checked) => {
                            if (checked) {
                              setSelectedRows(
                                new Set(namedDatabases().map((db) => db.name!)),
                              );
                            } else {
                              setSelectedRows(new Set<string>());
                            }
                          }}
                        />
                      </TableHead>
                      <TableHead>Name</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    <Show
                      when={props.config.databases.length > 0}
                      fallback={
                        <TableRow>
                          <TableCell colSpan={2}>
                            No linked databases.
                          </TableCell>
                        </TableRow>
                      }
                    >
                      <For each={props.config.databases}>
                        {(db) => (
                          <TableRow>
                            <TableCell>
                              <Checkbox
                                aria-label={`Select ${db.name ?? "unnamed database"}`}
                                checked={selectedRows().has(db.name ?? "")}
                                onChange={(checked) => {
                                  const next = new Set(selectedRows());
                                  if (checked) next.add(db.name ?? "");
                                  else next.delete(db.name ?? "");
                                  setSelectedRows(next);
                                }}
                              />
                            </TableCell>
                            <TableCell>{db.name ?? "<missing>"}</TableCell>
                          </TableRow>
                        )}
                      </For>
                    </Show>
                  </TableBody>
                </Table>
              </div>
            </div>
          </CardContent>
          <CardFooter>
            <div class="flex w-full justify-between gap-2">
              <Button
                variant="outline"
                type="button"
                disabled={pending()}
                onClick={() => {
                  setError();
                  setName("");
                  setLinkOpen(true);
                }}
              >
                <TbOutlineLink /> Link
              </Button>
              <Button
                variant="destructive"
                type="button"
                disabled={selected().length === 0 || pending()}
                onClick={() => {
                  setError();
                  setUnlinkOpen(true);
                }}
              >
                <TbOutlineUnlink /> Unlink
              </Button>
            </div>
          </CardFooter>
        </Card>
        <ImportExportCard />
      </div>
    </>
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
