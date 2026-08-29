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
  untrack,
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
    return "just now";
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

export function buildColumns(
  copyAccountId: (id: string) => Promise<void> = (id) =>
    copyToClipboard(id, true),
): ColumnDef<UserJson>[] {
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
                    void copyAccountId(id);
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
  const [deleting, setDeleting] = createSignal(false);
  const [error, setError] = createSignal<string>();

  return (
    <Dialog
      id="confirm"
      modal={true}
      open={dialogOpen()}
      onOpenChange={(open) => {
        setDialogOpen(open);
        if (open) setError(undefined);
      }}
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
              Cancel
            </Button>

            <Button
              variant="destructive"
              disabled={deleting()}
              onClick={() => {
                if (deleting()) return;
                setDeleting(true);
                (async () => {
                  try {
                    await deleteUser({ id: props.userId });
                    props.onDelete();
                    setDialogOpen(false);
                  } catch {
                    setError("Unable to delete account. Please try again.");
                  } finally {
                    setDeleting(false);
                  }
                })();
              }}
            >
              {deleting() ? "Deleting…" : "Delete account"}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>

      <Button
        type="button"
        class="bg-destructive text-destructive-foreground"
        onClick={() => {
          setError(undefined);
          setDialogOpen(true);
        }}
      >
        Delete
      </Button>
    </Dialog>
  );
}

function EditSheetContent(props: {
  user: UserJson;
  close: () => void;
  markDirty: (dirty?: boolean) => void;
  refetch: () => void;
}) {
  const [error, setError] = createSignal<string>();
  const [copyError, setCopyError] = createSignal<string>();
  const [tokenError, setTokenError] = createSignal<string>();
  const [copyingTokens, setCopyingTokens] = createSignal(false);
  const defaultValues = untrack((): UpdateUserRequest => ({
    id: props.user.id,
    email: props.user.email,
    unverified_email: props.user.unverified_email,
    username: props.user.username,
    password: null,
  }));
  const form = createForm(() => ({
    defaultValues,
    onSubmit: async ({ value }) => {
      setError(undefined);
      try {
        await updateUser(value);
        props.markDirty(false);
        props.refetch();
        props.close();
      } catch {
        setError("Unable to update account. Please try again.");
      }
    },
  }));

  form.useStore((state) => {
    const values = state.values;
    props.markDirty(
      values.email !== defaultValues.email ||
        values.unverified_email !== defaultValues.unverified_email ||
        values.username !== defaultValues.username ||
        values.password !== defaultValues.password,
    );
  });

  return (
    <SheetContainer>
      <SheetHeader>
        <SheetTitle>Edit account</SheetTitle>

        <Show when={error()}>
          <p class="text-destructive" role="alert">
            {error()}
          </p>
        </Show>

        <SheetDescription>
          Change a user's properties. Be careful
        </SheetDescription>

        <Show when={copyError()}>
          <p class="text-destructive" role="alert">
            {copyError()}
          </p>
        </Show>

        <Show when={tokenError()}>
          <p class="text-destructive" role="alert">
            {tokenError()}
          </p>
        </Show>
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
            <FixedWidthLabel>Identity</FixedWidthLabel>
            <div class="min-w-0">
              <div class="font-medium">
                {accountIdentity(props.user).primary}
              </div>
              <Show when={accountIdentity(props.user).secondary}>
                <div class="text-muted-foreground text-sm">
                  {accountIdentity(props.user).secondary}
                </div>
              </Show>
            </div>
          </div>
          <div class="flex w-full items-center justify-start gap-2">
            <FixedWidthLabel>Account ID</FixedWidthLabel>
            <code class="text-muted-foreground min-w-0 text-sm break-all">
              {props.user.id}
            </code>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => {
                setCopyError(undefined);
                void copyToClipboard(props.user.id, true).catch(() => {
                  setCopyError("Unable to copy account ID. Please try again.");
                });
              }}
            >
              Copy ID
            </Button>
          </div>
          <div class="flex w-full items-center justify-start gap-2">
            <FixedWidthLabel>Status</FixedWidthLabel>
            <div class="flex flex-wrap gap-1">
              <For each={accountStatuses(props.user)}>
                {(status) => (
                  <Badge variant={status.variant}>{status.label}</Badge>
                )}
              </For>
            </div>
          </div>
          <div class="flex w-full items-center justify-start gap-2">
            <FixedWidthLabel>Provider</FixedWidthLabel>
            <Badge>{accountProviderLabel(props.user.provider_id)}</Badge>
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
                  <div class="flex w-full justify-end gap-2 py-4">
                    <div class="flex gap-2">
                      <Show
                        when={!props.user.admin && !props.user.unverified_email}
                      >
                        <Button
                          type="button"
                          variant="outline"
                          aria-label="Copy login tokens"
                          title="Copy login tokens"
                          disabled={copyingTokens()}
                          onClick={(e) => {
                            e.stopPropagation();
                            if (copyingTokens()) return;

                            setCopyingTokens(true);
                            setTokenError(undefined);
                            (async () => {
                              try {
                                const loginResponse = await mintTokens({
                                  user: props.user.id,
                                });

                                await copyToClipboard(
                                  btoa(JSON.stringify(loginResponse)),
                                  false,
                                  "Copied tokens to clipboard",
                                );
                              } catch {
                                setTokenError(
                                  "Unable to copy login tokens. Please try again.",
                                );
                              } finally {
                                setCopyingTokens(false);
                              }
                            })();
                          }}
                        >
                          <TbOutlineCookie />
                          Copy login tokens
                        </Button>
                      </Show>

                      <Button
                        type="submit"
                        disabled={!state().canSubmit}
                        variant="default"
                      >
                        {state().isSubmitting ? "Saving…" : "Save changes"}
                      </Button>
                    </div>
                  </div>
                );
              }}
            />

            <section
              class="border-destructive/30 mt-2 border-t pt-3"
              aria-labelledby="danger-zone-title"
            >
              <h3 id="danger-zone-title" class="text-destructive font-medium">
                Danger zone
              </h3>
              <p class="text-muted-foreground mb-2 text-sm">
                Permanently delete this account.
              </p>
              <DeleteUserButton
                userId={props.user.id}
                name={accountIdentity(props.user).primary}
                onDelete={() => {
                  props.markDirty(false);
                  props.close();
                  props.refetch();
                }}
              />
            </section>

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
  const [editDirty, setEditDirty] = createSignal(false);
  const [addUserOpen, setAddUserOpen] = createSignal(false);
  const [copyError, setCopyError] = createSignal<string>();

  createEffect(() => {
    const selected = editUser();
    const loadedUsers = users.data;
    if (
      selected &&
      loadedUsers &&
      !loadedUsers.users.some((user) => user.id === selected.id) &&
      !editDirty()
    ) {
      setEditUser(undefined);
    }
  });

  const copyAccountId = async (id: string) => {
    setCopyError(undefined);
    try {
      await copyToClipboard(id, true);
    } catch {
      setCopyError("Unable to copy account ID. Please try again.");
    }
  };

  const accountsTable = createMemo(() => {
    return buildTable(
      {
        columns: buildColumns(copyAccountId),
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
            ? `${users.data.total_row_count ?? 0} accounts · Manage authentication identities and access`
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
              pageIndex: "0",
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
        <Show when={copyError()}>
          <Callout variant="error" role="alert" class="mt-4">
            {copyError()}
          </Callout>
        </Show>
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
                  onRowClick={(_idx, row) => {
                    setEditDirty(false);
                    setEditUser(row);
                  }}
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
              if (!isOpen) {
                setEditDirty(false);
                setEditUser(undefined);
              }
            },
          ]}
          children={(sheet) => (
            <SheetContent class={sheetMaxWidth}>
              <Show when={editUser()}>
                <EditSheetContent
                  user={editUser()!}
                  refetch={refetch}
                  close={sheet.close}
                  markDirty={(dirty) => {
                    setEditDirty(dirty ?? true);
                    sheet.markDirty(dirty);
                  }}
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
