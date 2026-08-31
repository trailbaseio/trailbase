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

export function formatBackupTimestamp(timestamp: unknown): string {
  let seconds: bigint;
  if (typeof timestamp === "bigint") seconds = timestamp;
  else if (typeof timestamp === "number" && Number.isSafeInteger(timestamp)) {
    seconds = BigInt(timestamp);
  } else if (typeof timestamp === "string" && /^-?\d+$/.test(timestamp)) {
    seconds = BigInt(timestamp);
  } else return "Unknown date";

  const milliseconds = seconds * 1000n;
  const maxMilliseconds = 8640000000000000n;
  if (milliseconds > maxMilliseconds || milliseconds < -maxMilliseconds) {
    return "Unknown date";
  }
  const date = new Date(Number(milliseconds));
  return Number.isNaN(date.getTime()) ? "Unknown date" : date.toLocaleString();
}

function Timestamp(props: { timestamp: bigint }) {
  return <div>{formatBackupTimestamp(props.timestamp)}</div>;
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
  const [operationError, setOperationError] = createSignal<
    { scope: "trigger" | "dialog"; message: string } | undefined
  >();
  let mounted = true;
  onCleanup(() => {
    mounted = false;
  });

  const timestampText = formatBackupTimestamp;
  const refetchAfterSuccess = async () => {
    if (mounted) await backupsList.refetch({ throwOnError: true });
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
      let refreshFailed = false;
      try {
        await refetchAfterSuccess();
      } catch {
        refreshFailed = true;
      }
      if (!mounted) return;
      setSelectedAction();
      showToast({
        title: action === "delete" ? "Backup deleted" : "Backup restored",
        variant: "success",
      });
      if (refreshFailed)
        setOperationError({
          scope: "trigger",
          message: "Backup changed, but the list could not refresh. Try again.",
        });
    } catch {
      if (mounted)
        setOperationError({
          scope: "dialog",
          message: "Backup operation failed. Try again.",
        });
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
      try {
        await refetchAfterSuccess();
      } catch {
        if (mounted)
          setOperationError({
            scope: "trigger",
            message:
              "Backup created, but the list could not refresh. Try again.",
          });
      }
      if (!mounted) return;
      showToast({ title: "Backup created", variant: "success" });
    } catch {
      if (mounted)
        setOperationError({
          scope: "trigger",
          message: "Backup operation failed. Try again.",
        });
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

        <Switch fallback={<p role="status">Loading...</p>}>
          <Match when={backupsList.isError}>
            <p role="alert">Unable to load backups. Try again.</p>
          </Match>

          <Match when={backupsList.isSuccess}>
            <Show
              when={(backupsList.data?.backups ?? []).length > 0}
              fallback={<p>No backups available.</p>}
            >
              <div class="overflow-x-auto rounded-md border">
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
                                  onClick={() => {
                                    setSelectedAction({
                                      action: "delete",
                                      timestamp: item.timestamp,
                                    });
                                    setOperationError();
                                  }}
                                >
                                  <TbOutlineTrash />
                                </IconButton>
                                <IconButton
                                  aria-label={`Restore backup from ${readableTime()}`}
                                  tooltip="Restore backup"
                                  disabled={!!pendingAction()}
                                  onClick={() => {
                                    setSelectedAction({
                                      action: "restore",
                                      timestamp: item.timestamp,
                                    });
                                    setOperationError();
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
        <Show when={operationError()?.scope === "trigger"}>
          <p role="alert">{operationError()?.message}</p>
        </Show>
      </CardContent>
      <Dialog
        open={selectedAction() !== undefined}
        onOpenChange={(open) => {
          if (!open && !pendingAction()) setSelectedAction();
        }}
      >
        <DialogContent closeDisabled={!!pendingAction()}>
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
          <Show when={operationError()?.scope === "dialog"}>
            <p role="alert">{operationError()?.message}</p>
          </Show>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={!!pendingAction()}
              onClick={() => {
                setSelectedAction();
                setOperationError();
              }}
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
              {pendingAction() === "delete"
                ? "Deleting…"
                : pendingAction() === "restore"
                  ? "Restoring…"
                  : "Confirm"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}

const listBackupsKey = ["admin", "backups"];
