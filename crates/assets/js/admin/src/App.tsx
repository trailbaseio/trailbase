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
import { $user } from "@/lib/fetch";
import { createWindowWidth } from "@/lib/signals";

const queryClient = new QueryClient();

function LeftNav(props: RouteSectionProps) {
  return (
    <>
      <div class="hide-scrollbars sticky h-dvh w-[58px] overflow-y-scroll">
        <VerticalNavBar location={props.location} />
      </div>

      <main class="absolute inset-0 left-[58px] h-dvh w-[calc(100vw-58px)] overflow-hidden">
        <ErrorBoundary>{props.children}</ErrorBoundary>
      </main>
    </>
  );
}

function TopNav(props: RouteSectionProps) {
  return (
    <>
      <div class="hide-scrollbars sticky h-[48px] w-screen overflow-y-scroll">
        <HorizontalNavBar location={props.location} />
      </div>

      <main class="absolute inset-0 top-[48px] h-[calc(100vh-48px)] w-screen overflow-hidden">
        <ErrorBoundary>{props.children}</ErrorBoundary>
      </main>
    </>
  );
}

function WrapWithNav(props: RouteSectionProps) {
  const width = createWindowWidth();
  const showTopNav = () => width() < 680;

  return (
    <Switch>
      <Match when={showTopNav()}>
        <TopNav {...props} />
      </Match>

      <Match when={!showTopNav()}>
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
