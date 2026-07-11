import { Switch, Match, For } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import { listBackups } from "@/lib/api/backups";

import { Card, CardContent, CardHeader } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

export function BackupSettings(props: {
  markDirty: () => void;
  postSubmit: () => void;
}) {
  const backupsList = useQuery(() => ({
    queryKey: listBackupsKey,
    queryFn: listBackups,
  }));

  return (
    <Card>
      <CardHeader>
        <h2>Backups</h2>
      </CardHeader>

      <CardContent>
        <Switch fallback="Loading...">
          <Match when={backupsList.isError}>
            {backupsList.error?.toString()}
          </Match>

          <Match when={backupsList.isSuccess}>
            <Table>
              <TableHeader>
                <TableHead>Name</TableHead>
              </TableHeader>

              <TableBody>
                <For each={backupsList.data?.backups ?? []}>
                  {(item) => {
                    return (
                      <TableRow>
                        <TableCell>{item.timestamp}</TableCell>
                      </TableRow>
                    );
                  }}
                </For>
              </TableBody>
            </Table>
          </Match>
        </Switch>
      </CardContent>
    </Card>
  );
}

const listBackupsKey = ["admin", "backups"];
