import { createSignal, Match, Show, Switch, Suspense } from "solid-js";
import { createWritableMemo } from "@solid-primitives/memo";
import type { Setter } from "solid-js";
import { useSearchParams } from "@solidjs/router";
import { TbRefresh, TbCrown, TbEdit, TbTrash } from "solid-icons/tb";
import type { DialogTriggerProps } from "@kobalte/core/dialog";
import { createForm } from "@tanstack/solid-form";
import { useQuery, useQueryClient } from "@tanstack/solid-query";
import { createColumnHelper } from "@tanstack/solid-table";
import type { ColumnDef, PaginationState } from "@tanstack/solid-table";

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
import { DataTable, safeParseInt } from "@/components/Table";
import { IconButton } from "@/components/IconButton";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { AddUser } from "@/components/accounts/AddUser";
import {
  buildTextFormField,
  buildSecretFormField,
} from "@/components/FormFields";
import { SafeSheet, SheetContainer } from "@/components/SafeSheet";

import { deleteUser, updateUser, fetchUsers } from "@/lib/api/user";
import { copyToClipboard } from "@/lib/utils";

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
      header: "id",
      accessorKey: "id",
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
      header: "email",
      accessorKey: "email",
    },
    {
      header: "verified",
      accessorKey: "verified",
    },
    columnHelper.accessor("id", {
      header: "admin",
      cell: (ctx) => (
        <div class="ml-[10px]">
          {ctx.row.original.admin ? <TbCrown size={20} /> : null}
        </div>
      ),
    }) as ColumnDef<UserJson>,
    columnHelper.display({
      header: "actions",
      cell: (ctx) => {
        return (
          <Show when={!ctx.row.original.admin}>
            <div class="flex gap-2">
              <IconButton
                tooltip="Edit user"
                onClick={() => setEditUser(ctx.row.original)}
              >
                <TbEdit size={20} />
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
                setDialogOpen(false);

                deleteUser({ id: props.userId })
                  .then(props.userRefetch)
                  .catch(console.error);
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
        <TbTrash size={20} />
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
      updateUser(value)
        // eslint-disable-next-line solid/reactivity
        .then(() => props.close())
        .catch(console.error);

      props.refetch();
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
  }>();
  // Reset when search params change
  const reset = () => {
    return [searchParams.pageSize, searchParams.filter];
  };
  const [pageIndex, setPageIndex] = createWritableMemo<number>(() => {
    reset();
    return 0;
  });
  const [cursors, setCursors] = createWritableMemo<string[]>(() => {
    reset();
    return [];
  });

  const pagination = (): PaginationState => {
    return {
      pageSize: safeParseInt(searchParams.pageSize) ?? 20,
      pageIndex: pageIndex(),
    };
  };

  const setFilter = (filter: string | undefined) => {
    setPageIndex(0);
    setSearchParams({
      ...searchParams,
      filter,
    });
  };

  // NOTE: admin user endpoint doesn't support offset, we have to cursor through
  // and cannot just jump to page N.
  const users = useQuery(() => ({
    queryKey: [
      "users",
      searchParams.filter,
      pagination().pageSize,
      pagination().pageIndex,
    ],
    queryFn: async () => {
      const p = pagination();
      const c = cursors();

      const response = await fetchUsers(
        searchParams.filter,
        pagination().pageSize,
        c[p.pageIndex - 1],
      );

      const cursor = response.cursor;
      if (cursor && p.pageIndex >= c.length) {
        setCursors([...c, cursor]);
      }

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

  const columns = () => buildColumns(setEditUser, refetch);

  return (
    <div class="h-full">
      <Header
        title="Accounts"
        left={
          <IconButton onClick={refetch}>
            <TbRefresh size={18} />
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
          placeholder={`Filter Query, e.g. 'email ~ "admin@%" && verified = TRUE'`}
        />

        <Suspense fallback={<div>Loading...</div>}>
          <Switch>
            <Match when={users.isError}>
              <span>Error: {users.error?.toString()}</span>
            </Match>

            <Match when={users.isLoading}>
              <span>Loading</span>
            </Match>

            <Match when={users.data}>
              <div class="w-full space-y-2.5">
                <DataTable
                  columns={columns}
                  data={() => users.data!.users}
                  rowCount={Number(users.data!.total_row_count ?? -1)}
                  pagination={pagination()}
                  onPaginationChange={(
                    p:
                      | PaginationState
                      | ((old: PaginationState) => PaginationState),
                  ) => {
                    function setPagination({
                      pageSize,
                      pageIndex,
                    }: PaginationState) {
                      const current = pagination();
                      if (current.pageSize !== pageSize) {
                        setSearchParams({
                          ...searchParams,
                          pageSize,
                        });
                        return;
                      }

                      if (current.pageIndex != pageIndex) {
                        setPageIndex(pageIndex);
                      }
                    }

                    if (typeof p === "function") {
                      setPagination(p(pagination()));
                    } else {
                      setPagination(p);
                    }
                  }}
                />
              </div>

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
            </Match>
          </Switch>
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
