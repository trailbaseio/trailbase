import { Switch, Match, For, Show, createSignal, onCleanup } from "solid-js";
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

function Timestamp(props: { timestamp: bigint }) {
  return <div>{new Date(Number(props.timestamp)).toLocaleString()}</div>;
}

export function BackupSettings(_props: {
  setDirty: (dirty: boolean) => void;
  postSubmit: () => void;
}) {
  const backupsList = useQuery(() => ({
    queryKey: listBackupsKey,
    queryFn: listBackups,
  }));
  const config = createConfigQuery();
  const [selectedAction, setSelectedAction] = createSignal<
    { action: "delete" | "restore"; timestamp: bigint } | undefined
  >();
  const [pendingAction, setPendingAction] = createSignal<string>();
  const [operationError, setOperationError] = createSignal<string>();
  let mounted = true;
  onCleanup(() => {
    mounted = false;
  });

  const timestampText = (timestamp: bigint) =>
    new Date(Number(timestamp)).toLocaleString();
  const refetchAfterSuccess = async () => {
    if (mounted) await backupsList.refetch();
  };
  const runOperation = async (
    action: "delete" | "restore",
    timestamp: bigint,
  ) => {
    if (pendingAction()) return;
    setPendingAction(action);
    setOperationError();
    try {
      if (action === "delete") await deleteBackups([timestamp]);
      else await restoreBackup(timestamp);
      if (!mounted) return;
      await refetchAfterSuccess();
      if (!mounted) return;
      setSelectedAction();
      showToast({
        title: action === "delete" ? "Backup deleted" : "Backup restored",
        variant: "success",
      });
    } catch {
      if (mounted) setOperationError("Backup operation failed. Try again.");
    } finally {
      if (mounted) setPendingAction();
    }
  };
  const trigger = async () => {
    if (pendingAction()) return;
    setPendingAction("trigger");
    setOperationError();
    try {
      await triggerBackup();
      if (!mounted) return;
      await refetchAfterSuccess();
      if (!mounted) return;
      showToast({ title: "Backup created", variant: "success" });
    } catch {
      if (mounted) setOperationError("Backup operation failed. Try again.");
    } finally {
      if (mounted) setPendingAction();
    }
  };

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
            <p role="alert">Unable to load backups. Try again.</p>
          </Match>

          <Match when={backupsList.isSuccess}>
            <Show
              when={(backupsList.data?.backups ?? []).length > 0}
              fallback={<p>No backups available.</p>}
            >
              <div class="rounded-md border">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Time</TableHead>
                      <TableHead>
                        <span class="flex justify-center">Actions</span>
                      </TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    <For each={backupsList.data?.backups ?? []}>
                      {(item) => {
                        const readableTime = () =>
                          timestampText(item.timestamp);
                        return (
                          <TableRow>
                            <TableCell>
                              <Timestamp timestamp={item.timestamp} />
                            </TableCell>
                            <TableCell>
                              <div class="flex gap-2">
                                <IconButton
                                  aria-label={`Delete backup from ${readableTime()}`}
                                  tooltip="Delete backup"
                                  disabled={!!pendingAction()}
                                  onClick={() =>
                                    setSelectedAction({
                                      action: "delete",
                                      timestamp: item.timestamp,
                                    })
                                  }
                                >
                                  <TbOutlineTrash />
                                </IconButton>
                                <IconButton
                                  aria-label={`Restore backup from ${readableTime()}`}
                                  tooltip="Restore backup"
                                  disabled={!!pendingAction()}
                                  onClick={() =>
                                    setSelectedAction({
                                      action: "restore",
                                      timestamp: item.timestamp,
                                    })
                                  }
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
            </Show>
            <div class="flex justify-end">
              <Button
                variant="outline"
                disabled={!!pendingAction()}
                onClick={trigger}
              >
                <TbOutlineDeviceFloppy />
                {pendingAction() === "trigger"
                  ? "Triggering Backup…"
                  : "Trigger Backup"}
              </Button>
            </div>
          </Match>
        </Switch>
        <Show when={operationError()}>
          <p role="alert">{operationError()}</p>
        </Show>
      </CardContent>
      <Dialog
        open={selectedAction() !== undefined}
        onOpenChange={(open) => {
          if (!open && !pendingAction()) setSelectedAction();
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {selectedAction() &&
                `${selectedAction()!.action === "delete" ? "Delete" : "Restore"} backup from ${timestampText(selectedAction()!.timestamp)}`}
            </DialogTitle>
            <DialogDescription>
              {selectedAction() &&
                `Backup from ${timestampText(selectedAction()!.timestamp)}.`}
            </DialogDescription>
          </DialogHeader>
          <Show when={operationError()}>
            <p role="alert">{operationError()}</p>
          </Show>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={!!pendingAction()}
              onClick={() => setSelectedAction()}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              disabled={!!pendingAction()}
              onClick={() => {
                const selected = selectedAction();
                if (selected)
                  void runOperation(selected.action, selected.timestamp);
              }}
            >
              {pendingAction() ? "Working…" : "Confirm"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}

const listBackupsKey = ["admin", "backups"];
