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
import { TbRefresh, TbCrown, TbCheck, TbClipboardCopy } from "solid-icons/tb";
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

function buildColumns(): ColumnDef<UserJson>[] {
  // NOTE: the headers are lower-case to match the column names and don't confuse when trying to use the filter bar.
  return [
    {
      accessorKey: "id",
      size: 340,
      cell: (ctx) => {
        const userId = ctx.row.original.id;
        return (
          <div class="flex items-center gap-2">
            <Button
              variant="ghost"
              size="icon"
              onClick={(e) => {
                e.stopPropagation();
                copyToClipboard(userId, true);
              }}
            >
              <TbClipboardCopy />
            </Button>

            {userId}
          </div>
        );
      },
    },
    {
      accessorKey: "email",
      size: 220,
    },
    {
      accessorKey: "verified",
      cell: (ctx) => {
        return (
          <Show when={ctx.row.original.verified}>
            <div class="flex justify-center pr-6">
              <TbCheck />
            </div>
          </Show>
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
              <div class="flex justify-center pr-4">
                <img class="size-[20px]" src={oauthAsset!} />
              </div>
            </Match>

            <Match when={providerId > 0n}>{`${providerId}`}</Match>
          </Switch>
        );
      },
    },
  ];
}

function DeleteUserButton(props: {
  userId: string;
  email: string;
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

          <form.Field name="password">
            {buildSecretFormField({
              label: () => <FixedWidthLabel children="Password" />,
            })}
          </form.Field>

          <form.Field name="verified">
            {(field) => (
              <div class="flex w-full items-center justify-start gap-2">
                <FixedWidthLabel>Verified</FixedWidthLabel>
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
                <div class="flex w-full justify-between gap-2 py-4">
                  <DeleteUserButton
                    userId={props.user.id}
                    email={props.user.email}
                    onDelete={() => {
                      props.close();
                      props.refetch();
                    }}
                  />

                  <Button
                    type="submit"
                    disabled={!state().canSubmit}
                    variant="default"
                  >
                    {state().isSubmitting ? "..." : "Submit"}
                  </Button>
                </div>
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
