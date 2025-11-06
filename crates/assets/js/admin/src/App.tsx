import { lazy, Match, Switch } from "solid-js";
import type { Component } from "solid-js";
import { Router, Route, type RouteSectionProps } from "@solidjs/router";
import { useStore } from "@nanostores/solid";
import { QueryClient, QueryClientProvider } from "@tanstack/solid-query";

import { TablePage } from "@/components/tables/TablesPage";
import { AccountsPage } from "@/components/accounts/AccountsPage";
import { LoginPage } from "@/components/auth/LoginPage";
import { SettingsPage } from "@/components/settings/SettingsPage";
import { IndexPage } from "@/components/IndexPage";
import { VerticalNavBar, HorizontalNavBar } from "@/components/NavBar";

import { ErrorBoundary } from "@/components/ErrorBoundary";
import { $user } from "@/lib/client";
import { createIsMobile } from "@/lib/signals";

const queryClient = new QueryClient();

function LeftNav(props: RouteSectionProps) {
  return (
    <>
      <div class="hide-scrollbars sticky h-dvh w-[58px] overflow-hidden">
        <VerticalNavBar location={props.location} />
      </div>

      <main class="absolute inset-0 left-[58px] h-dvh w-[calc(100vw-58px)] overflow-x-hidden overflow-y-auto">
        <ErrorBoundary>{props.children}</ErrorBoundary>
      </main>
    </>
  );
}

function TopNav(props: RouteSectionProps) {
  return (
    <>
      <HorizontalNavBar height={48} location={props.location} />

      <main class="max-h-[calc(100vh-48px)] w-screen">
        <ErrorBoundary>{props.children}</ErrorBoundary>
      </main>
    </>
  );
}

function WrapWithNav(props: RouteSectionProps) {
  const isMobile = createIsMobile();

  return (
    <Switch>
      <Match when={isMobile()}>
        <TopNav {...props} />
      </Match>

      <Match when={!isMobile()}>
        <LeftNav {...props} />
      </Match>
    </Switch>
  );
}

function NotFoundPage() {
  return <h1>Not Found</h1>;
}

const LazyEditorPage = lazy(() => import("@/components/editor/EditorPage"));
const LazyLogsPage = lazy(() => import("@/components/logs/LogsPage"));
const LazyErdPage = lazy(() => import("@/components/erd/ErdPage"));

const App: Component = () => {
  const user = useStore($user);

  return (
    <QueryClientProvider client={queryClient}>
      {user() ? (
        <Router base={"/_/admin"} root={WrapWithNav}>
          <Route path="/" component={IndexPage} />
          <Route path="/table/:table?" component={TablePage} />
          <Route path="/auth" component={AccountsPage} />
          <Route path="/editor" component={LazyEditorPage} />
          <Route path="/erd" component={LazyErdPage} />
          <Route path="/logs" component={LazyLogsPage} />
          <Route path="/settings/:group?" component={SettingsPage} />

          {/* fallback: */}
          <Route path="*" component={NotFoundPage} />
        </Router>
      ) : (
        <ErrorBoundary>
          <LoginPage />
        </ErrorBoundary>
      )}
    </QueryClientProvider>
  );
};

export default App;
