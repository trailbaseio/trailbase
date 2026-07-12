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

import { createConfigQuery } from "@/lib/api/config";

import { showToast } from "@/components/ui/toast";
import { Button } from "@/components/ui/button";
import { IconButton } from "@/components/IconButton";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

function Timestamp(props: { timestamp: BigInt }) {
  const time = (): Date => new Date(Number(props.timestamp));

  return <div>{time().toLocaleString()}</div>;
}

export function BackupSettings(_props: {
  markDirty: () => void;
  postSubmit: () => void;
}) {
  const backupsList = useQuery(() => ({
    queryKey: listBackupsKey,
    queryFn: listBackups,
  }));
  const config = createConfigQuery();

  return (
    <Card>
      <CardHeader>
        <h2>Backups</h2>
      </CardHeader>

      <CardContent class="flex flex-col gap-4">
        <p class="text-sm">
          You can backup and restore all registered databases. Additionally,
          periodic backups can be configured via the Jobs tab. The oldest
          backups exceeding a configurable rolling window are cleaned up
          automatically. Note that the window size can currently only be
          configured using the text configuration. Current window size:{" "}
          {Number(config.data?.config?.server?.backupWindowSize ?? 5)}.
        </p>

        <Switch fallback="Loading...">
          <Match when={backupsList.isError}>
            {backupsList.error?.toString()}
          </Match>

          <Match when={backupsList.isSuccess}>
            <div class="rounded-md border">
              <Table>
                <TableHeader>
                  <TableHead>Time</TableHead>
                  <TableHead class="w-[120px]">
                    <span class="flex justify-center">Actions</span>
                  </TableHead>
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
                              <IconButton
                                tooltip="Delete backup."
                                onClick={() => {
                                  (async () => {
                                    await deleteBackups([item.timestamp]);
                                    await backupsList.refetch();
                                  })();
                                }}
                              >
                                <TbOutlineTrash />
                              </IconButton>

                              <IconButton
                                tooltip="Restore backup."
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
                              </IconButton>
                            </div>
                          </TableCell>
                        </TableRow>
                      );
                    }}
                  </For>
                </TableBody>
              </Table>
            </div>

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
                <TbOutlineDeviceFloppy />
                Trigger Backup
              </Button>
            </div>
          </Match>
        </Switch>
      </CardContent>
    </Card>
  );
}

const listBackupsKey = ["admin", "backups"];
