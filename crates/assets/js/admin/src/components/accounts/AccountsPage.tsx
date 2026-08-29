import {
  createEffect,
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
    // Keep sub-minute values from displaying misleading 0 or 60 seconds.
    const truncated = Math.trunc(differenceSeconds);
    const seconds =
      truncated === 0 && differenceSeconds !== 0
        ? differenceSeconds > 0
          ? 1
          : -1
        : Math.max(-59, Math.min(59, truncated));
    return new Intl.RelativeTimeFormat(locale, { numeric: "always" }).format(
      seconds,
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
  name: string;
  onDelete: () => void;
}) {
  const [dialogOpen, setDialogOpen] = createSignal(false);
  const [error, setError] = createSignal<string>();

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
          <span class="font-bold">{props.name}</span>?
        </p>

        <Show when={error()}>
          <p class="text-destructive" role="alert">
            {error()}
          </p>
        </Show>

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
                    props.onDelete();
                    setDialogOpen(false);
                  } catch (error) {
                    setError(
                      error instanceof Error ? error.message : String(error),
                    );
                  }
                })();
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
  const [error, setError] = createSignal<string>();
  const form = createForm(() => ({
    defaultValues: {
      id: props.user.id,
      email: props.user.email,
      unverified_email: props.user.unverified_email,
      username: props.user.username,
      password: null,
    } as UpdateUserRequest,
    onSubmit: async ({ value }) => {
      setError(undefined);
      try {
        await updateUser(value);
        props.close();
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        props.refetch();
      }
    },
  }));

  form.useStore((state) => {
    if (state.isDirty && !state.isSubmitted) {
      props.markDirty();
    }
  });

  return (
    <SheetContainer>
      <SheetHeader>
        <SheetTitle>Edit User</SheetTitle>

        <Show when={error()}>
          <p class="text-destructive" role="alert">
            {error()}
          </p>
        </Show>

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
                      name={accountIdentity(props.user).primary}
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
                          aria-label="Copy login tokens"
                          title="Copy login tokens"
                          onClick={(e) => {
                            e.stopPropagation();

                            (async () => {
                              const loginResponse = await mintTokens({
                                user: props.user.id,
                              });

                              copyToClipboard(
                                btoa(JSON.stringify(loginResponse)),
                                false,
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

export function AccountToolbar(props: {
  advanced: boolean;
  onModeChange: (advanced: boolean) => void;
  children: JSX.Element;
}) {
  return (
    <div class="w-full space-y-2">
      <div class="flex gap-2">
        <Button
          variant={props.advanced ? "ghost" : "outline"}
          aria-pressed={!props.advanced}
          onClick={() => props.onModeChange(false)}
        >
          Search accounts
        </Button>
        <Button
          variant={props.advanced ? "outline" : "ghost"}
          aria-pressed={props.advanced}
          onClick={() => props.onModeChange(true)}
        >
          Advanced account filter
        </Button>
      </div>
      {props.children}
    </div>
  );
}

export function AccountsPage() {
  const [searchParams, setSearchParams] = useSearchParams<{
    search?: string;
    filter?: string;
    advanced?: string;
    pageSize?: string;
    pageIndex?: string;
  }>();
  const pagination = (): PaginationState => {
    return {
      pageSize: safeParseInt(searchParams.pageSize) ?? 20,
      pageIndex: safeParseInt(searchParams.pageIndex) ?? 0,
    };
  };

  const setFilter = (filter: string | undefined) =>
    setSearchParams({ ...searchParams, filter, pageIndex: "0" });
  const setSearch = (search: string | undefined) =>
    setSearchParams({ ...searchParams, search, pageIndex: "0" });

  const [sorting, setSorting] = createSignal<SortingState>([]);

  // NOTE: admin user endpoint doesn't support offset, we have to cursor through
  // and cannot just jump to page N.
  const users = useQuery(() => ({
    queryKey: [
      "users",
      searchParams.filter,
      searchParams.search,
      searchParams.advanced,
      pagination().pageSize,
      pagination().pageIndex,
      sorting(),
    ],
    queryFn: async () => {
      const p = pagination();
      const s = sorting();

      const response = await fetchUsers(
        searchParams.advanced === "true" ? searchParams.filter : undefined,
        p.pageSize,
        p.pageIndex,
        formatSortingAsOrder(s),
        searchParams.advanced === "true" ? undefined : searchParams.search,
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
  const [addUserOpen, setAddUserOpen] = createSignal(false);

  createEffect(() => {
    const selected = editUser();
    const loadedUsers = users.data;
    if (
      selected &&
      loadedUsers &&
      !loadedUsers.users.some((user) => user.id === selected.id)
    ) {
      setEditUser(undefined);
    }
  });

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

  const hasQuery = Boolean(
    searchParams.advanced === "true"
      ? searchParams.filter
      : searchParams.search,
  );
  const emptyState = () =>
    hasQuery ? (
      <div class="p-4 text-center">
        <p>No accounts match the current search or filter.</p>
        <Button
          variant="outline"
          onClick={() => {
            setSearch(undefined);
            setFilter(undefined);
          }}
        >
          Clear search/filter
        </Button>
      </div>
    ) : (
      <div class="p-4 text-center">
        <p>No accounts yet.</p>
      </div>
    );

  return (
    <div class="flex h-full min-h-0 flex-col">
      <Header
        title="Accounts"
        description={
          users.data
            ? `${users.data.total_row_count ?? 0} accounts`
            : "Manage authentication identities and access"
        }
        left={
          <IconButton
            onClick={refetch}
            aria-label="Refresh accounts"
            title="Refresh accounts"
          >
            <TbOutlineRefresh />
          </IconButton>
        }
        right={
          <Button data-add-account onClick={() => setAddUserOpen(true)}>
            Add account
          </Button>
        }
      />
      <div class="min-h-0 flex-1 overflow-auto p-4">
        <AccountToolbar
          advanced={searchParams.advanced === "true"}
          onModeChange={(advanced) =>
            setSearchParams({
              ...searchParams,
              advanced: advanced ? "true" : "false",
            })
          }
        >
          <FilterBar
            label={
              searchParams.advanced === "true"
                ? "Advanced account filter"
                : "Search accounts"
            }
            initial={
              searchParams.advanced === "true"
                ? searchParams.filter
                : searchParams.search
            }
            onSubmit={(value) => {
              const current =
                searchParams.advanced === "true"
                  ? searchParams.filter
                  : searchParams.search;
              if (value === current) refetch();
              else if (searchParams.advanced === "true") setFilter(value);
              else setSearch(value);
            }}
            placeholder={
              searchParams.advanced === "true"
                ? `Filter, e.g.: 'email ~ "admin@%" && admin = TRUE'`
                : "Search accounts…"
            }
          />
        </AccountToolbar>
        <Suspense fallback={<div>Loading accounts…</div>}>
          <Switch>
            <Match when={users.isError}>
              <Callout variant="error" role="alert">
                Unable to load accounts.
                <Button variant="outline" onClick={refetch}>
                  Retry
                </Button>
              </Callout>
            </Match>
            <Match when={true}>
              <div class="mt-4">
                <Table
                  table={accountsTable()}
                  loading={users.isLoading}
                  dense={true}
                  paginationPosition="bottom"
                  emptyState={emptyState()}
                  onRowClick={(_idx, row) => setEditUser(row)}
                />
              </div>
            </Match>
          </Switch>
        </Suspense>
        <SafeSheet
          open={[addUserOpen, setAddUserOpen]}
          children={(sheet) => (
            <SheetContent class={sheetMaxWidth}>
              <AddUser userRefetch={refetch} {...sheet} />
            </SheetContent>
          )}
        />
        <SafeSheet
          open={[
            () => editUser() !== undefined,
            (isOpen: boolean | ((value: boolean) => boolean)) => {
              if (!isOpen) setEditUser(undefined);
            },
          ]}
          children={(sheet) => (
            <SheetContent class={sheetMaxWidth}>
              <Show when={editUser()}>
                <EditSheetContent
                  user={editUser()!}
                  refetch={refetch}
                  {...sheet}
                />
              </Show>
            </SheetContent>
          )}
        />
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
