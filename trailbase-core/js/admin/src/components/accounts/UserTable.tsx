import {
  createResource,
  createSignal,
  Match,
  Show,
  Switch,
  Suspense,
} from "solid-js";
import type { Setter } from "solid-js";
import { createForm } from "@tanstack/solid-form";
import { TbCrown, TbEdit, TbTrash } from "solid-icons/tb";
import type { DialogTriggerProps } from "@kobalte/core/dialog";

import {
  type ColumnDef,
  createColumnHelper,
  PaginationState,
} from "@tanstack/solid-table";
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

import { Checkbox } from "@/components/ui/checkbox";
import { DataTable } from "@/components/Table";
import { IconButton } from "@/components/IconButton";
import { Label } from "@/components/ui/label";
import { AddUser } from "@/components/accounts/AddUser";
import {
  deleteUser,
  updateUser,
  fetchUsers,
  type FetchUsersArgs,
} from "@/lib/user";
import type { UpdateUserRequest, UserJson } from "@/lib/bindings";
import {
  buildTextFormField,
  buildSecretFormField,
} from "@/components/FormFields";
import { SafeSheet, SheetContainer } from "@/components/SafeSheet";

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
        const userId = ctx.row.original.id;
        return (
          <div class="flex gap-2">
            <IconButton
              tooltip="Edit user"
              onClick={() => setEditUser(ctx.row.original)}
            >
              <TbEdit size={20} />
            </IconButton>

            <IconButton
              class="bg-destructive text-white"
              tooltip="Delete user"
              onClick={() => {
                deleteUser(userId).then(userRefetch).catch(console.error);
              }}
            >
              <TbTrash size={20} />
            </IconButton>
          </div>
        );
      },
    }),
  ];
}

function EditSheetContent(props: {
  user: UserJson;
  close: () => void;
  markDirty: () => void;
  refetch: () => void;
}) {
  const form = createForm<UpdateUserRequest>(() => ({
    defaultValues: {
      id: props.user.id,
      email: props.user.email,
      password: null,
      verified: props.user.verified,
    },
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
        onSubmit={(e) => {
          e.preventDefault();
          e.stopPropagation();
          form.handleSubmit();
        }}
      >
        <div class="flex flex-col items-center gap-4 py-4">
          <form.Field name={"email"}>
            {buildTextFormField({
              label: textLabel("E-mail"),
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

export function UserTable() {
  const [filter, setFilter] = createSignal<string | undefined>();
  const [pagination, setPagination] = createSignal<PaginationState>({
    pageSize: 20,
    pageIndex: 0,
  });
  const cursors: string[] = [];

  const buildFetchArgs = (): FetchUsersArgs => ({
    pageSize: pagination().pageSize,
    pageIndex: pagination().pageIndex,
    cursors: cursors,
    filter: filter(),
  });

  const [users, { refetch }] = createResource(buildFetchArgs, fetchUsers);
  const [editUser, setEditUser] = createSignal<UserJson | undefined>();

  const columns = () => buildColumns(setEditUser, refetch);

  return (
    <Suspense fallback={<div>Loading...</div>}>
      <Switch>
        <Match when={users.error}>
          <span>Error: {users.error}</span>
        </Match>

        <Match when={users()}>
          <div class="flex flex-col items-end gap-4">
            <FilterBar
              initial={filter()}
              onSubmit={(value: string) => {
                if (value === filter()) {
                  refetch();
                } else {
                  setFilter(value);
                }
              }}
              example='e.g. "email[like]=%@foo.com"'
            />

            <div class="w-full space-y-2.5">
              <DataTable
                columns={columns}
                data={() => users()?.users}
                rowCount={Number(users()?.total_row_count)}
                initialPagination={pagination()}
                onPaginationChange={setPagination}
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
          </div>
        </Match>
      </Switch>
    </Suspense>
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
