import { Separator } from "@/components/ui/separator";

import { UserTable } from "./UserTable";

export function AccountsPage() {
  return (
    <>
      <h1 class="m-4 text-accent-600">Users</h1>

      <Separator />

      <div class="m-4">
        <UserTable />
      </div>
    </>
  );
}
