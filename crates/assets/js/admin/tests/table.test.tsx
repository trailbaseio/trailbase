import { fireEvent, render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";

import { Table, buildTable } from "@/components/Table";

type RowData = { name: string };

function makeTable(data: RowData[], paginated = false) {
  return buildTable({
    columns: [{ accessorKey: "name", header: "Name" }],
    data,
    ...(paginated
      ? {
          rowCount: data.length,
          pagination: { pageIndex: 0, pageSize: 10 },
          onPaginationChange: () => undefined,
        }
      : {}),
  });
}

describe("Table presentation", () => {
  it("renders a supplied empty state", () => {
    const result = render(() => (
      <Table
        table={makeTable([])}
        loading={false}
        emptyState={<button>Insert first row</button>}
      />
    ));

    expect(
      result.getByRole("button", { name: "Insert first row" }),
    ).toBeInTheDocument();
    expect(result.queryByText("Empty")).toBeNull();
  });

  it("places pagination after the grid when requested", () => {
    const result = render(() => (
      <Table
        table={makeTable([{ name: "Ada" }], true)}
        loading={false}
        paginationPosition="bottom"
      />
    ));
    const grid = result.container.querySelector("table");
    const pagination = result.getByText(/page 1 of 1/).closest("div.flex");
    expect(
      result.getByRole("button", { name: "Go to last page" }),
    ).toBeInTheDocument();

    expect(grid).not.toBeNull();
    expect(
      grid!.compareDocumentPosition(pagination as Node) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("activates clickable rows with Enter", async () => {
    const onRowClick = vi.fn();
    const result = render(() => (
      <Table
        table={makeTable([{ name: "Ada" }])}
        loading={false}
        onRowClick={onRowClick}
        dense
      />
    ));
    const row = result.getByText("Ada").closest("tr")!;

    expect(row).toHaveAttribute("tabindex", "0");
    await fireEvent.keyDown(row, { key: "Enter" });
    expect(onRowClick).toHaveBeenCalledWith(0, { name: "Ada" }, row);

    await fireEvent.click(row);
    await fireEvent.keyDown(row, { key: " " });
    expect(onRowClick).toHaveBeenCalledTimes(3);
    expect(onRowClick.mock.calls[1][2]).toBe(row);
    expect(onRowClick.mock.calls[2][2]).toBe(row);
  });
});
