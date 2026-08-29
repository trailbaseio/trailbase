import { fireEvent, render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { EditorSidebar } from "@/components/editor/EditorPage";
import { SidebarProvider } from "@/components/ui/sidebar";

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
