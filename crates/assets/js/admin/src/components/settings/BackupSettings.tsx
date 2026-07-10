import { Switch, Match, For } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import {
  listBackups,
  triggerBackup,
  restoreBackup,
  deleteBackups,
} from "@/lib/api/backups";
import {
  TbOutlineRestore,
  TbOutlineTrash,
  TbOutlineDeviceFloppy,
} from "solid-icons/tb";

import { showToast } from "@/components/ui/toast";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

function Timestamp(props: { timestamp: string }) {
  const time = (): Date => new Date(Date.parse(props.timestamp));

  return <div>{time().toLocaleString()}</div>;
}

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
                <TableHead>Time</TableHead>
                <TableHead>Action</TableHead>
              </TableHeader>

              <TableBody>
                <For each={backupsList.data?.backups ?? []}>
                  {(item) => {
                    return (
                      <TableRow>
                        <TableCell>
                          <Timestamp timestamp={item.timestamp} />
                        </TableCell>
                        <TableCell>
                          <div class="flex gap-2">
                            <Button
                              size="icon"
                              variant="outline"
                              onClick={() => {
                                (async () => {
                                  await deleteBackups([item.timestamp]);
                                  await backupsList.refetch();
                                })();
                              }}
                            >
                              <TbOutlineTrash />
                            </Button>

                            <Button
                              size="icon"
                              variant="outline"
                              onClick={() => {
                                (async () => {
                                  await restoreBackup(item.timestamp);

                                  showToast({
                                    title: "restored backup",
                                    variant: "success",
                                  });
                                })();
                              }}
                            >
                              <TbOutlineRestore />
                            </Button>
                          </div>
                        </TableCell>
                      </TableRow>
                    );
                  }}
                </For>
              </TableBody>
            </Table>

            <div class="flex justify-end">
              <Button
                variant="outline"
                onClick={() => {
                  (async () => {
                    await triggerBackup();
                    await backupsList.refetch();
                  })();
                }}
              >
                Trigger New Backup <TbOutlineDeviceFloppy />
              </Button>
            </div>
          </Match>
        </Switch>
      </CardContent>
    </Card>
  );
}

const listBackupsKey = ["admin", "backups"];
