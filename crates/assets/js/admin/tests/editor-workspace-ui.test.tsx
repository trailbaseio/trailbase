import { fireEvent, render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  AttachedDatabaseSelect,
  EditorSidebar,
  QueryActionBar,
} from "@/components/editor/EditorPage";
import { SidebarProvider } from "@/components/ui/sidebar";

describe("attached database selector", () => {
  it("shows the selected database count", () => {
    const result = render(() => (
      <AttachedDatabaseSelect
        options={["other"]}
        value={["other"]}
        onChange={vi.fn()}
      />
    ));

    const trigger = result.getByRole("button", {
      name: /Attached databases/,
    });
    expect(trigger).toHaveTextContent("Attached databases · 1");

    result.unmount();

    const empty = render(() => (
      <AttachedDatabaseSelect
        options={["other"]}
        value={[]}
        onChange={vi.fn()}
      />
    ));
    expect(empty.getByRole("button")).toHaveTextContent(
      "Attached databases · 0",
    );
    empty.unmount();
  });
});

describe("query actions", () => {
  it("exposes clear save and execute actions", async () => {
    const save = vi.fn();
    const execute = vi.fn();
    const result = render(() => (
      <QueryActionBar
        busy={false}
        mobile={false}
        onSave={save}
        onExecute={execute}
      />
    ));

    await fireEvent.click(result.getByRole("button", { name: "Save query" }));
    await fireEvent.click(
      result.getByRole("button", { name: "Execute query" }),
    );
    expect(save).toHaveBeenCalledOnce();
    expect(execute).toHaveBeenCalledOnce();
    result.unmount();
  });

  it("prevents duplicate execution while running", () => {
    const result = render(() => (
      <QueryActionBar
        busy
        mobile={false}
        onSave={vi.fn()}
        onExecute={vi.fn()}
      />
    ));

    expect(
      result.getByRole("button", { name: "Execute query" }),
    ).toBeDisabled();
    expect(result.getByText("Running…")).toBeInTheDocument();
    result.unmount();
  });
});

describe("saved query explorer", () => {
  beforeEach(() => {
    window.matchMedia = () =>
      ({
        matches: false,
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
      }) as unknown as MediaQueryList;
  });

  it("filters saved queries and recovers from no results", async () => {
    const result = render(() => (
      <SidebarProvider cookieName="editor-test:state">
        <EditorSidebar
          selected={0}
          setSelected={vi.fn()}
          dirty={false}
          deleteScriptByIdx={vi.fn()}
        />
      </SidebarProvider>
    ));

    const search = result.getByRole("searchbox", {
      name: "Search saved queries",
    });
    expect(result.getByText("Select Users")).toBeInTheDocument();

    await fireEvent.input(search, { target: { value: "missing" } });
    expect(result.getByText("No saved queries match")).toBeInTheDocument();

    await fireEvent.click(result.getByRole("button", { name: "Clear search" }));
    expect(search).toHaveValue("");
    expect(result.getByText("Select Users")).toBeInTheDocument();
    result.unmount();
  });
});
