import { UserTable } from "@/components/accounts/UserTable";
import { Header } from "@/components/Header";

export function AccountsPage() {
  return (
    <>
      <Header title="Accounts" />

      <div class="m-4">
        <UserTable />
      </div>
    </>
  );
}
