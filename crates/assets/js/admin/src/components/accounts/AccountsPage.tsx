import {
  createMemo,
  createSignal,
  Match,
  Show,
  Switch,
  Suspense,
} from "solid-js";
import type { Setter } from "solid-js";
import { useSearchParams } from "@solidjs/router";
import { TbRefresh, TbCrown, TbEdit, TbTrash } from "solid-icons/tb";
import type { DialogTriggerProps } from "@kobalte/core/dialog";
import { createForm } from "@tanstack/solid-form";
import { useQuery, useQueryClient } from "@tanstack/solid-query";
import { createColumnHelper } from "@tanstack/solid-table";
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

import { Header } from "@/components/Header";
import { Table, buildTable } from "@/components/Table";
import { IconButton } from "@/components/IconButton";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { AddUser } from "@/components/accounts/AddUser";
import {
  buildTextFormField,
  buildSecretFormField,
} from "@/components/FormFields";
import { SafeSheet, SheetContainer } from "@/components/SafeSheet";
import { assets } from "@/components/settings/AuthSettings";

import { deleteUser, updateUser, fetchUsers } from "@/lib/api/user";
import { copyToClipboard, safeParseInt } from "@/lib/utils";
import { formatSortingAsOrder } from "@/lib/list";

import type { UpdateUserRequest } from "@bindings/UpdateUserRequest";
import type { UserJson } from "@bindings/UserJson";

const columnHelper = createColumnHelper<UserJson>();

function buildColumns(
  setEditUser: Setter<UserJson | undefined>,
  userRefetch: () => void,
): ColumnDef<UserJson>[] {
  // NOTE: the headers are lower-case to match the column names and don't confuse when trying to use the filter bar.
  return [
    {
      accessorKey: "id",
      size: 300,
      cell: (ctx) => {
        const userId = ctx.row.original.id;
        return (
          <div
            class="hover:text-gray-600"
            onClick={() => copyToClipboard(userId)}
          >
            {userId}
          </div>
        );
      },
    },
    {
      accessorKey: "email",
      size: 240,
    },
    {
      accessorKey: "verified",
    },
    {
      header: "OAuth",
      enableSorting: false,
      cell: (ctx) => {
        const providerId = ctx.row.original.provider_id;
        const oauthAsset =
          providerId > 0n ? assets.get(Number(providerId)) : undefined;

        return (
          <Switch>
            <Match when={oauthAsset !== undefined}>
              <div class="flex justify-center">
                <img class="size-[20px]" src={oauthAsset!} />
              </div>
            </Match>

            <Match when={providerId > 0n}>{`${providerId}`}</Match>
          </Switch>
        );
      },
    },
    {
      accessorKey: "admin",
      cell: (ctx) => (
        <div class="flex justify-center">
          {ctx.row.original.admin ? <TbCrown size={18} /> : null}
        </div>
      ),
    },
    columnHelper.display({
      header: "actions",
      enableSorting: false,
      cell: (ctx) => {
        return (
          <Show when={!ctx.row.original.admin}>
            <div class="flex gap-2">
              <IconButton
                tooltip="Edit user"
                onClick={() => setEditUser(ctx.row.original)}
              >
                <TbEdit />
              </IconButton>

              <DeleteUserButton
                userId={ctx.row.original.id}
                email={ctx.row.original.email}
                userRefetch={userRefetch}
              />
            </div>
          </Show>
        );
      },
    }),
  ];
}

function DeleteUserButton(props: {
  userId: string;
  email: string;
  userRefetch: () => void;
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
          <span class="font-bold">{props.email}</span>?
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
                    props.userRefetch();
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

      <IconButton
        class="bg-destructive text-white"
        tooltip="Delete user"
        onClick={() => setDialogOpen(true)}
      >
        <TbTrash />
      </IconButton>
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
      password: null,
      verified: props.user.verified,
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
          <form.Field name={"email"}>
            {buildTextFormField({
              label: textLabel("Email"),
              type: "email",
            })}
          </form.Field>

          <form.Field name="password">
            {buildSecretFormField({
              label: textLabel("Password"),
            })}
          </form.Field>

          <form.Field name="verified">
            {(field) => (
              <div class="flex w-full items-center justify-end gap-2">
                <Label>Verified</Label>
                <Checkbox
                  checked={field().state.value ?? false}
                  onChange={field().handleChange}
                />
              </div>
            )}
          </form.Field>
        </div>

        <SheetFooter>
          <form.Subscribe
            selector={(state) => ({
              canSubmit: state.canSubmit,
              isSubmitting: state.isSubmitting,
            })}
            children={(state) => {
              return (
                <Button
                  type="submit"
                  disabled={!state().canSubmit}
                  variant="default"
                >
                  {state().isSubmitting ? "..." : "Submit"}
                </Button>
              );
            }}
          />
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
        columns: buildColumns(setEditUser, refetch),
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
    <div class="h-full">
      <Header
        title="Accounts"
        left={
          <IconButton onClick={refetch}>
            <TbRefresh />
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
          placeholder={`Filter, e.g.: 'email ~ "admin@%" && verified = TRUE'`}
        />

        <Suspense fallback={<div>Loading...</div>}>
          <Switch>
            <Match when={users.isError}>
              <span>Error: {users.error?.toString()}</span>
            </Match>

            <Match when={users.isLoading}>
              <div class="w-full space-y-2.5">
                <Table
                  table={accountsTable()}
                  loading={true}
                  onRowClick={undefined}
                />
              </div>
            </Match>

            <Match when={users.isSuccess}>
              <div class="w-full space-y-2.5">
                <Table
                  table={accountsTable()}
                  loading={false}
                  onRowClick={undefined}
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

function textLabel(label: string) {
  return () => (
    <div class="w-32">
      <Label>{label}</Label>
    </div>
  );
}

const sheetMaxWidth = "sm:max-w-[520px]";
