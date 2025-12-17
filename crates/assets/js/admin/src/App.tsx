import { createSignal, lazy, Match, Show, Switch } from "solid-js";
import type { Component } from "solid-js";
import { Router, Route, type RouteSectionProps } from "@solidjs/router";
import { useStore } from "@nanostores/solid";
import { QueryClient, QueryClientProvider } from "@tanstack/solid-query";

import { TablePage } from "@/components/tables/TablesPage";
import { AccountsPage } from "@/components/accounts/AccountsPage";
import { LoginPage } from "@/components/auth/LoginPage";
import { SettingsPage } from "@/components/settings/SettingsPage";
import { IndexPage } from "@/components/IndexPage";
import {
  VerticalNavbar,
  HorizontalNavbar,
  NavbarContext,
} from "@/components/Navbar";
import { ErrorBoundary } from "@/components/ErrorBoundary";

import { $user } from "@/lib/client";
import { createIsMobile } from "@/lib/signals";

const queryClient = new QueryClient();

function LeftNav(props: RouteSectionProps) {
  return (
    <>
      <div class="hide-scrollbars sticky h-dvh w-[58px] overflow-hidden">
        <VerticalNavbar location={props.location} />
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
      <HorizontalNavbar height={48} location={props.location} />

      <main class="max-h-[calc(100vh-48px)] w-screen">
        <ErrorBoundary>{props.children}</ErrorBoundary>
      </main>
    </>
  );
}

function WrapWithNav(props: RouteSectionProps) {
  const isMobile = createIsMobile();
  const [dirty, setDirty] = createSignal(false);

  const contextValue = {
    dirty,
    setDirty,
  };

  return (
    <NavbarContext.Provider value={contextValue}>
      <Switch>
        <Match when={isMobile()}>
          <TopNav {...props} />
        </Match>

        <Match when={!isMobile()}>
          <LeftNav {...props} />
        </Match>
      </Switch>
    </NavbarContext.Provider>
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

  const Login = () => (
    <ErrorBoundary>
      <LoginPage />
    </ErrorBoundary>
  );

  function isAdmin() {
    const u = user();
    return u !== undefined && u.admin === true;
  }

  return (
    <QueryClientProvider client={queryClient}>
      <Show when={isAdmin()} fallback={<Login />}>
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
      </Show>
    </QueryClientProvider>
  );
};

export default App;
