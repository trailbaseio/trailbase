import { lazy, onMount, Show, createSignal } from "solid-js";
import logo from "@/assets/logo_104.webp";
import type { Component } from "solid-js";
import { Router, Route, type RouteSectionProps } from "@solidjs/router";
import { useStore } from "@nanostores/solid";
import { QueryClient, QueryClientProvider } from "@tanstack/solid-query";
import { TablePage } from "@/components/tables/TablesPage";
import { AccountsPage } from "@/components/accounts/AccountsPage";
import { WasmPage } from "@/components/wasm/WasmPage";
import { LoginPage } from "@/components/auth/LoginPage";
import { SettingsPage } from "@/components/settings/SettingsPage";
import { IndexPage } from "@/components/IndexPage";
import { Navbar, NavbarContext } from "@/components/Navbar";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import {
  SidebarProvider,
  Sidebar,
  SidebarInset,
  SidebarRail,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { $user } from "@/lib/client";
import { initializeTheme } from "@/lib/theme";
const queryClient = new QueryClient();
function WrapWithNav(props: RouteSectionProps) {
  const [dirty, setDirty] = createSignal(false);
  return (
    <NavbarContext.Provider value={{ dirty, setDirty }}>
      <SidebarProvider>
        <Sidebar collapsible="icon">
          <Navbar />
          <SidebarRail />
        </Sidebar>
        <SidebarInset>
          <header class="flex h-12 items-center border-b px-4">
            <SidebarTrigger class="md:hidden" />
            <div class="text-foreground ml-2 flex min-w-0 items-center gap-2 text-sm font-semibold">
              <img src={logo} width="24" height="24" alt="" />
              <span class="truncate">TrailBase</span>
            </div>
          </header>
          <main class="min-h-0 flex-1 overflow-auto">
            <ErrorBoundary>{props.children}</ErrorBoundary>
          </main>
        </SidebarInset>
      </SidebarProvider>
    </NavbarContext.Provider>
  );
}
function NotFoundPage() {
  return <h1>Not Found</h1>;
}
const LazyEditorPage = lazy(() => import("@/components/editor/EditorPage"));
const LazyLogsPage = lazy(() => import("@/components/logs/LogsPage"));
const LazyErdPage = lazy(() => import("@/components/erd/ErdPage"));
const LazyOpenApiPage = lazy(() => import("@/components/openapi/OpenApiPage"));
const App: Component = () => {
  const user = useStore($user);
  onMount(() => initializeTheme());
  const isAdmin = () => user()?.admin === true;
  return (
    <QueryClientProvider client={queryClient}>
      <Show
        when={isAdmin()}
        fallback={
          <ErrorBoundary>
            <LoginPage />
          </ErrorBoundary>
        }
      >
        <Router base="/_/admin" root={WrapWithNav}>
          <Route path="/" component={IndexPage} />
          <Route path="/table/:table?" component={TablePage} />
          <Route path="/auth" component={AccountsPage} />
          <Route path="/wasm/:name?" component={WasmPage} />
          <Route path="/editor" component={LazyEditorPage} />
          <Route path="/erd" component={LazyErdPage} />
          <Route path="/logs" component={LazyLogsPage} />
          <Route path="/openapi" component={LazyOpenApiPage} />
          <Route path="/settings/:group?" component={SettingsPage} />
          <Route path="*" component={NotFoundPage} />
        </Router>
      </Show>
    </QueryClientProvider>
  );
};
export default App;
