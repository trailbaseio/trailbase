import {
  createMemo,
  createSignal,
  JSX,
  Match,
  Show,
  Switch,
  Suspense,
} from "solid-js";
import { useSearchParams } from "@solidjs/router";
import {
  TbOutlineBroadcast,
  TbOutlineClipboardCopy,
  TbOutlineCookie,
  TbOutlineCrown,
  TbOutlineQuestionMark,
  TbOutlineRefresh,
  TbOutlineUser,
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
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

import { Callout } from "@/components/ui/callout";
import { Header } from "@/components/Header";
import { Table, buildTable } from "@/components/Table";
import { IconButton } from "@/components/IconButton";
import { Label } from "@/components/ui/label";
import { AddUser } from "@/components/accounts/AddUser";
import {
  buildTextFormField,
  buildSecretFormField,
} from "@/components/FormFields";
import { SafeSheet, SheetContainer } from "@/components/SafeSheet";
import { assets } from "@/components/settings/AuthSettings";

import { mintTokens } from "@/lib/api/mint";
import { deleteUser, updateUser, fetchUsers } from "@/lib/api/user";
import { copyToClipboard, safeParseInt } from "@/lib/utils";
import { formatSortingAsOrder } from "@/lib/list";

import type { UpdateUserRequest } from "@bindings/UpdateUserRequest";
import type { UserJson } from "@bindings/UserJson";

function buildColumns(): ColumnDef<UserJson>[] {
  // NOTE: the headers are lower-case to match the column names and don't confuse when trying to use the filter bar.
  return [
    {
      accessorKey: "id",
      header: () => {
        return <div class="ml-3">id</div>;
      },
      size: 320,
      cell: (ctx) => {
        const { id } = ctx.row.original;
        return (
          <div class="flex items-center gap-2">
            <Button
              variant="ghost"
              size="icon"
              onClick={(e) => {
                e.stopPropagation();
                copyToClipboard(id, true);
              }}
            >
              <TbOutlineClipboardCopy />
            </Button>

            <span class="font-mono text-xs">{id}</span>
          </div>
        );
      },
    },
    {
      accessorKey: "username",
      minSize: 180,
      cell: (ctx) => {
        return (
          <span class="line-clamp-2 text-left text-ellipsis">
            {ctx.row.original.username}
          </span>
        );
      },
    },
    {
      accessorKey: "email",
      header: "email (? = 'unverified_email')",
      minSize: 260,
      cell: (ctx) => {
        const { email, unverified_email } = ctx.row.original;

        return (
          <Switch>
            <Match when={unverified_email}>
              <div class="flex w-full items-center gap-2">
                <TbOutlineQuestionMark />

                <span class="text-muted-foreground w-[calc(100%-24px)] text-wrap">
                  {unverified_email}
                </span>
              </div>
            </Match>

            <Match when={true}>{email}</Match>
          </Switch>
        );
      },
    },
    {
      accessorKey: "admin",
      size: 60,
      cell: (ctx) => (
        <div class="px-2">
          {ctx.row.original.admin ? <TbOutlineCrown size={18} /> : null}
        </div>
      ),
    },
    {
      id: "OAuth",
      header: () => (
        <div class="ml-3 flex items-center">
          <Tooltip>
            <TooltipTrigger>
              <TbOutlineBroadcast />
            </TooltipTrigger>

            <TooltipContent>OAuth provider</TooltipContent>
          </Tooltip>
        </div>
      ),
      size: 60,
      enableSorting: false,
      cell: (ctx) => {
        const providerId = ctx.row.original.provider_id;
        const oauthAsset =
          providerId > 0n ? assets.get(Number(providerId)) : undefined;

        return (
          <Switch>
            <Match when={oauthAsset !== undefined}>
              <div class="px-2">
                <img class="size-[20px]" src={oauthAsset!} />
              </div>
            </Match>

            <Match when={providerId > 0n}>{`${providerId}`}</Match>
          </Switch>
        );
      },
    },
    {
      accessorKey: "updated",
      cell: (ctx) => {
        return new Date(Number(ctx.row.original.updated) * 1000).toUTCString();
      },
    },
    {
      accessorKey: "created",
      cell: (ctx) => {
        return new Date(Number(ctx.row.original.created) * 1000).toUTCString();
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
        class="bg-destructive text-white"
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
            <span class="text-sm text-gray-600">{props.user.id}</span>
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
    <div class="size-full overflow-y-auto">
      <Header
        title="Accounts"
        description={
          <Show when={users.data}>
            {(data) => <span>{Number(data().total_row_count)} accounts</span>}
          </Show>
        }
        left={
          <IconButton onClick={refetch}>
            <TbOutlineRefresh />
          </IconButton>
        }
        right={
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
                        <span>Add</span>
                        <TbOutlineUser />
                      </Button>
                    )}
                  />
                </>
              );
            }}
          />
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
