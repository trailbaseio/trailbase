import { lazy, type Component } from "solid-js";
import { Router, Route, type RouteSectionProps } from "@solidjs/router";
import { useStore } from "@nanostores/solid";

import { TablesPage } from "@/components/tables/TablesPage";
import { AccountsPage } from "@/components/accounts/AccountsPage";
import { LoginPage } from "@/components/auth/LoginPage";
import { SettingsPages } from "@/components/settings/SettingsPage";
import { IndexPage } from "@/components/IndexPage";
import { NavBar } from "@/components/NavBar";

import { ErrorBoundary } from "@/components/ErrorBoundary";
import { $user } from "@/lib/fetch";

function Layout(props: RouteSectionProps) {
  return (
    <div>
      <div class="absolute inset-0 w-[58px] flex flex-col">
        <NavBar location={props.location} />
      </div>

      <main class="absolute inset-0 left-[58px] overflow-x-hidden">
        <ErrorBoundary>{props.children}</ErrorBoundary>
      </main>
    </div>
  );
}

const LazyEditorPage = lazy(() => import("@/components/editor/EditorPage"));
const LazyLogsPage = lazy(() => import("@/components/logs/LogsPage"));

const App: Component = () => {
  const user = useStore($user);

  return (
    <>
      {user() ? (
        <ErrorBoundary>
          <Router base={"/_/admin"} root={Layout}>
            <Route path="/" component={IndexPage} />
            <Route path="/tables" component={TablesPage} />
            <Route path="/auth" component={AccountsPage} />
            <Route path="/editor" component={LazyEditorPage} />
            <Route path="/logs" component={LazyLogsPage} />
            <Route path="/settings">
              <SettingsPages />
            </Route>

            {/* fallback: */}
            <Route path="*" component={() => <h1>Not Found</h1>} />
          </Router>
        </ErrorBoundary>
      ) : (
        <ErrorBoundary>
          <LoginPage />
        </ErrorBoundary>
      )}
    </>
  );
};

export default App;
