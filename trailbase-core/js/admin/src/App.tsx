import { lazy, type Component } from "solid-js";
import { Router, Route, type RouteSectionProps } from "@solidjs/router";
import { useStore } from "@nanostores/solid";

import { TablePage } from "@/components/tables/TablesPage";
import { AccountsPage } from "@/components/accounts/AccountsPage";
import { LoginPage } from "@/components/auth/LoginPage";
import { SettingsPage } from "@/components/settings/SettingsPage";
import { IndexPage } from "@/components/IndexPage";
import { NavBar } from "@/components/NavBar";

import { ErrorBoundary } from "@/components/ErrorBoundary";
import { $user } from "@/lib/fetch";

function Layout(props: RouteSectionProps) {
  return (
    <ErrorBoundary>
      <div class="hide-scrollbars sticky flex h-dvh w-[58px] flex-col overflow-y-scroll">
        <NavBar location={props.location} />
      </div>

      <main class="absolute inset-0 left-[58px] h-dvh w-[calc(100%-58px)] overflow-hidden">
        {props.children}
      </main>
    </ErrorBoundary>
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
            <Route path="/table/:table?" component={TablePage} />
            <Route path="/auth" component={AccountsPage} />
            <Route path="/editor" component={LazyEditorPage} />
            <Route path="/logs" component={LazyLogsPage} />
            <Route path="/settings/:group?" component={SettingsPage} />

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
