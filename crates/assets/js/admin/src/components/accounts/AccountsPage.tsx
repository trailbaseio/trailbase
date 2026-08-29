import {
  createMemo,
  createSignal,
  For,
  JSX,
  Match,
  Show,
  Switch,
  Suspense,
} from "solid-js";
import { useSearchParams } from "@solidjs/router";
import {
  TbOutlineRefresh,
  TbOutlineClipboardCopy,
  TbOutlineCookie,
} from "solid-icons/tb";
import type { DialogTriggerProps } from "@kobalte/core/dialog";
import { createForm } from "@tanstack/solid-form";
import { useQuery, useQueryClient } from "@tanstack/solid-query";
import type {
  ColumnDef,
  PaginationState,
  SortingState,
} from "@tanstack/solid-table";

import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { FilterBar } from "@/components/FilterBar";
import {
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "@/components/ui/sheet";

import { Callout } from "@/components/ui/callout";
import { Header } from "@/components/Header";
import { Table, buildTable } from "@/components/Table";
import { IconButton } from "@/components/IconButton";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { AddUser } from "@/components/accounts/AddUser";
import {
  buildTextFormField,
  buildSecretFormField,
} from "@/components/FormFields";
import { SafeSheet, SheetContainer } from "@/components/SafeSheet";
import { assets } from "@/components/settings/AuthSettings";
import { OAuthProviderId } from "@proto/config";

import { mintTokens } from "@/lib/api/mint";
import { deleteUser, updateUser, fetchUsers } from "@/lib/api/user";
import { copyToClipboard, safeParseInt } from "@/lib/utils";
import { formatSortingAsOrder } from "@/lib/list";

import type { UpdateUserRequest } from "@bindings/UpdateUserRequest";
import type { UserJson } from "@bindings/UserJson";

type AccountIdentity = { primary: string; secondary?: string };

type AccountStatus = {
  label: string;
  variant: "default" | "success" | "warning";
};

export function accountIdentity(user: UserJson): AccountIdentity {
  const identities = [user.email, user.username, user.unverified_email].filter(
    (value): value is string => value !== null && value !== "",
  );
  const primary = identities[0] ?? "Unnamed account";
  const secondary = identities.find((value) => value !== primary);

  return { primary, secondary };
}

export function accountStatuses(user: UserJson): AccountStatus[] {
  const statuses: AccountStatus[] = [];

  if (user.admin) {
    statuses.push({ label: "Admin", variant: "default" });
  }
  statuses.push(
    user.unverified_email
      ? { label: "Pending verification", variant: "warning" }
      : { label: "Verified", variant: "success" },
  );

  return statuses;
}

const providerLabels = new Map<number, string>([
  [OAuthProviderId.TEST, "Test"],
  [OAuthProviderId.OIDC0, "OIDC"],
  [OAuthProviderId.APPLE, "Apple"],
  [OAuthProviderId.DISCORD, "Discord"],
  [OAuthProviderId.GITLAB, "GitLab"],
  [OAuthProviderId.GOOGLE, "Google"],
  [OAuthProviderId.FACEBOOK, "Facebook"],
  [OAuthProviderId.MICROSOFT, "Microsoft"],
  [OAuthProviderId.TWITCH, "Twitch"],
  [OAuthProviderId.YANDEX, "Yandex"],
  [OAuthProviderId.GITHUB, "GitHub"],
]);

export function accountProviderLabel(providerId: bigint): string {
  if (providerId === 0n) {
    return "Password";
  }

  return providerLabels.get(Number(providerId)) ?? `OAuth ${providerId}`;
}

export function formatAccountTime(
  timestampSeconds: bigint,
  nowMs: number,
  locale?: string,
): string {
  const differenceMs = Number(timestampSeconds) * 1000 - nowMs;
  const differenceSeconds = differenceMs / 1000;
  const absoluteSeconds = Math.abs(differenceSeconds);

  if (absoluteSeconds < 60) {
    return new Intl.RelativeTimeFormat(locale, { numeric: "always" }).format(
      Math.round(differenceSeconds),
      "second",
    );
  }

  const units: Array<[Intl.RelativeTimeFormatUnit, number]> = [
    ["minute", 60],
    ["hour", 60 * 60],
    ["day", 60 * 60 * 24],
    ["month", 60 * 60 * 24 * 30],
    ["year", 60 * 60 * 24 * 365],
  ];
  let unit: Intl.RelativeTimeFormatUnit = "minute";
  let unitSeconds = 60;

  for (const [candidate, seconds] of units) {
    if (absoluteSeconds >= seconds) {
      unit = candidate;
      unitSeconds = seconds;
    }
  }

  return new Intl.RelativeTimeFormat(locale, { numeric: "always" }).format(
    Math.round(differenceSeconds / unitSeconds),
    unit,
  );
}

export function shortAccountId(id: string): string {
  return id.length > 12 ? `${id.slice(0, 8)}…${id.slice(-4)}` : id;
}

export function buildColumns(): ColumnDef<UserJson>[] {
  return [
    {
      header: "Account",
      accessorKey: "id",
      minSize: 260,
      cell: (ctx) => {
        const { id } = ctx.row.original;
        const identity = accountIdentity(ctx.row.original);

        return (
          <div class="flex min-w-0 items-center gap-2">
            <div class="min-w-0">
              <div class="truncate font-medium">{identity.primary}</div>
              <Show when={identity.secondary}>
                <div class="text-muted-foreground truncate text-sm">
                  {identity.secondary}
                </div>
              </Show>
              <div class="text-muted-foreground flex items-center gap-1 font-mono text-xs">
                <span>{shortAccountId(id)}</span>
                <Button
                  variant="ghost"
                  size="icon"
                  class="size-6"
                  aria-label="Copy account ID"
                  title="Copy account ID"
                  onClick={(e) => {
                    e.stopPropagation();
                    copyToClipboard(id, true);
                  }}
                >
                  <TbOutlineClipboardCopy size={14} />
                </Button>
              </div>
            </div>
          </div>
        );
      },
    },
    {
      header: "Status",
      accessorKey: "status",
      enableSorting: false,
      cell: (ctx) => (
        <div class="flex flex-wrap gap-1">
          <For each={accountStatuses(ctx.row.original)}>
            {(status) => <Badge variant={status.variant}>{status.label}</Badge>}
          </For>
        </div>
      ),
    },
    {
      header: "Provider",
      accessorKey: "provider",
      enableSorting: false,
      cell: (ctx) => {
        const providerId = ctx.row.original.provider_id;
        const providerLabel = accountProviderLabel(providerId);
        const oauthAsset =
          providerId > 0n ? assets.get(Number(providerId)) : undefined;

        return (
          <div class="flex items-center gap-2">
            <Show when={oauthAsset}>
              <img
                class="size-5"
                src={oauthAsset}
                alt={providerLabel}
                title={providerLabel}
              />
            </Show>
            <span>{providerLabel}</span>
          </div>
        );
      },
    },
    {
      header: "Created",
      accessorKey: "created",
      cell: (ctx) => {
        const timestamp = ctx.row.original.created;
        return (
          <time
            dateTime={new Date(Number(timestamp) * 1000).toISOString()}
            title={new Date(Number(timestamp) * 1000).toUTCString()}
          >
            {formatAccountTime(timestamp, Date.now())}
          </time>
        );
      },
    },
    {
      header: "Last updated",
      accessorKey: "updated",
      cell: (ctx) => {
        const timestamp = ctx.row.original.updated;
        return (
          <time
            dateTime={new Date(Number(timestamp) * 1000).toISOString()}
            title={new Date(Number(timestamp) * 1000).toUTCString()}
          >
            {formatAccountTime(timestamp, Date.now())}
          </time>
        );
      },
    },
  ];
}

function DeleteUserButton(props: {
  userId: string;
  email: string | null;
  username: string | null;
  onDelete: () => void;
}) {
  const [dialogOpen, setDialogOpen] = createSignal(false);

  return (
    <Dialog
      id="confirm"
      modal={true}
      open={dialogOpen()}
      onOpenChange={setDialogOpen}
    >
      <DialogContent>
        <DialogTitle>Confirmation</DialogTitle>

        <p>
          Are you sure you want to permanently delete{" "}
          <span class="font-bold">{props.email ?? props.username}</span>?
        </p>

        <DialogFooter>
          <div class="flex w-full justify-between">
            <Button variant="outline" onClick={() => setDialogOpen(false)}>
              Back
            </Button>

            <Button
              variant="destructive"
              onClick={() => {
                (async () => {
                  try {
                    await deleteUser({ id: props.userId });
                  } finally {
                    props.onDelete();
                  }
                })();

                setDialogOpen(false);
              }}
            >
              Delete
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>

      <Button
        class="bg-destructive text-destructive-foreground"
        onClick={() => setDialogOpen(true)}
      >
        Delete
      </Button>
    </Dialog>
  );
}

function EditSheetContent(props: {
  user: UserJson;
  close: () => void;
  markDirty: () => void;
  refetch: () => void;
}) {
  const form = createForm(() => ({
    defaultValues: {
      id: props.user.id,
      email: props.user.email,
      unverified_email: props.user.unverified_email,
      username: props.user.username,
      password: null,
    } as UpdateUserRequest,
    onSubmit: async ({ value }) => {
      try {
        await updateUser(value);
        props.close();
      } finally {
        props.refetch();
      }
    },
  }));

  return (
    <SheetContainer>
      <SheetHeader>
        <SheetTitle>Edit User</SheetTitle>

        <SheetDescription>
          Change a user's properties. Be careful
        </SheetDescription>
      </SheetHeader>

      <form
        method="dialog"
        onSubmit={(e: SubmitEvent) => {
          e.preventDefault();

          form.handleSubmit();
        }}
      >
        <div class="flex flex-col items-center gap-4 py-4">
          <div class="flex w-full items-center justify-start gap-2">
            <FixedWidthLabel>id</FixedWidthLabel>
            <span class="text-muted-foreground text-sm">{props.user.id}</span>
          </div>

          <form.Field name={"email"}>
            {buildTextFormField({
              label: () => <FixedWidthLabel children="Email" />,
              type: "email",
            })}
          </form.Field>

          <form.Field name="unverified_email">
            {buildTextFormField({
              label: () => <FixedWidthLabel children="Unverified Email" />,
              type: "email",
            })}
          </form.Field>

          <form.Field name={"username"}>
            {buildTextFormField({
              label: () => <FixedWidthLabel children="Username" />,
              type: "text",
            })}
          </form.Field>

          <form.Field name="password">
            {buildSecretFormField({
              label: () => <FixedWidthLabel children="Password" />,
            })}
          </form.Field>
        </div>

        <SheetFooter>
          <div class="flex w-full flex-col">
            <form.Subscribe
              selector={(state) => ({
                canSubmit: state.canSubmit,
                isSubmitting: state.isSubmitting,
              })}
              children={(state) => {
                return (
                  <div class="flex w-full justify-between gap-2 py-4">
                    <DeleteUserButton
                      userId={props.user.id}
                      email={props.user.email}
                      username={props.user.username}
                      onDelete={() => {
                        props.close();
                        props.refetch();
                      }}
                    />

                    <div class="flex gap-2">
                      <Show
                        when={!props.user.admin && !props.user.unverified_email}
                      >
                        <Button
                          variant="outline"
                          size="icon"
                          onClick={(e) => {
                            e.stopPropagation();

                            (async () => {
                              const loginResponse = await mintTokens({
                                user: props.user.id,
                              });

                              copyToClipboard(
                                btoa(JSON.stringify(loginResponse)),
                                true,
                                "Copied tokens to clipboard",
                              );
                            })();
                          }}
                        >
                          <TbOutlineCookie />
                        </Button>
                      </Show>

                      <Button
                        type="submit"
                        disabled={!state().canSubmit}
                        variant="default"
                      >
                        {state().isSubmitting ? "..." : "Submit"}
                      </Button>
                    </div>
                  </div>
                );
              }}
            />

            <Callout class="text-sm">
              The admin status can only be toggled using the CLI to prevent
              abuse.
            </Callout>
          </div>
        </SheetFooter>
      </form>
    </SheetContainer>
  );
}

export function AccountsPage() {
  const [searchParams, setSearchParams] = useSearchParams<{
    filter?: string;
    pageSize?: string;
    pageIndex?: string;
  }>();
  const pagination = (): PaginationState => {
    return {
      pageSize: safeParseInt(searchParams.pageSize) ?? 20,
      pageIndex: safeParseInt(searchParams.pageIndex) ?? 0,
    };
  };

  const setFilter = (filter: string | undefined) => {
    setSearchParams({
      ...searchParams,
      filter,
      // Reset
      pageIndex: "0",
    });
  };

  const [sorting, setSorting] = createSignal<SortingState>([]);

  // NOTE: admin user endpoint doesn't support offset, we have to cursor through
  // and cannot just jump to page N.
  const users = useQuery(() => ({
    queryKey: [
      "users",
      searchParams.filter,
      pagination().pageSize,
      pagination().pageIndex,
      sorting(),
    ],
    queryFn: async () => {
      const p = pagination();
      const s = sorting();

      const response = await fetchUsers(
        searchParams.filter,
        p.pageSize,
        p.pageIndex,
        formatSortingAsOrder(s),
      );

      return response;
    },
  }));
  const client = useQueryClient();
  const refetch = () => {
    client.invalidateQueries({
      queryKey: ["users"],
    });
  };

  const [editUser, setEditUser] = createSignal<UserJson | undefined>();

  const accountsTable = createMemo(() => {
    return buildTable(
      {
        columns: buildColumns(),
        data: users.data?.users ?? [],
        rowCount: Number(users.data?.total_row_count ?? -1),
        pagination: pagination(),
        onPaginationChange: (s: PaginationState) => {
          setSearchParams({
            ...searchParams,
            pageIndex: s.pageIndex,
            pageSize: s.pageSize,
          });
        },
      },
      {
        manualSorting: true,
        state: {
          sorting: sorting(),
        },
        onSortingChange: setSorting,
      },
    );
  });

  return (
    <div>
      <Header
        title="Accounts"
        left={
          <IconButton onClick={refetch}>
            <TbOutlineRefresh />
          </IconButton>
        }
      />

      <div class="flex flex-col items-end gap-4 p-4">
        <FilterBar
          initial={searchParams.filter}
          onSubmit={(value: string) => {
            if (value === searchParams.filter) {
              refetch();
            } else {
              setFilter(value);
            }
          }}
          placeholder={`Filter, e.g.: 'email ~ "admin@%" && admin = TRUE'`}
        />

        <Suspense fallback={<div>Loading...</div>}>
          <Switch>
            <Match when={users.isError}>
              <span>Error: {users.error?.toString()}</span>
            </Match>

            <Match when={true}>
              <div class="w-full space-y-2.5">
                <Table
                  table={accountsTable()}
                  loading={users.isLoading}
                  onRowClick={(_idx: number, row: UserJson) => {
                    setEditUser(row);
                  }}
                />
              </div>
            </Match>
          </Switch>

          <SafeSheet
            children={(sheet) => {
              return (
                <>
                  <SheetContent class={sheetMaxWidth}>
                    <AddUser userRefetch={refetch} {...sheet} />
                  </SheetContent>

                  <SheetTrigger
                    as={(props: DialogTriggerProps) => (
                      <Button
                        variant="outline"
                        class="flex gap-2"
                        onClick={() => {}}
                        {...props}
                      >
                        Add User
                      </Button>
                    )}
                  />
                </>
              );
            }}
          />

          {/* WARN: This might open multiple sheets or at least scrims for each row */}
          <SafeSheet
            open={[
              () => editUser() !== undefined,
              (isOpen: boolean | ((value: boolean) => boolean)) => {
                if (!isOpen) {
                  setEditUser(undefined);
                }
              },
            ]}
            children={(sheet) => {
              return (
                <SheetContent class={sheetMaxWidth}>
                  <Show when={editUser()}>
                    <EditSheetContent
                      user={editUser()!}
                      refetch={refetch}
                      {...sheet}
                    />
                  </Show>
                </SheetContent>
              );
            }}
          />
        </Suspense>
      </div>
    </div>
  );
}

function FixedWidthLabel(props: { children: JSX.Element }) {
  return (
    <div class="w-32">
      <Label class="w-32">{props.children}</Label>
    </div>
  );
}

const sheetMaxWidth = "sm:max-w-[520px]";
