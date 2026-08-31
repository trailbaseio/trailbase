import { render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@antv/x6", () => {
  class FakeGraph {
    static last: FakeGraph;
    zoom = vi.fn(() => 1);
    zoomTo = vi.fn();

    constructor() {
      FakeGraph.last = this;
    }

    static registerPortLayout() {}
    static registerNode() {}
    dispose() {}
    getCells() {
      return [];
    }
    getNodes() {
      return [];
    }
    getEdges() {
      return [];
    }
    resetCells() {}
    on() {}
    zoomToFit() {}
  }

  class FakeEdge {
    constructor() {}
  }

  return { Graph: FakeGraph, Shape: { Edge: FakeEdge }, Edge: FakeEdge };
});

import { Graph } from "@antv/x6";
import { ErdGraph } from "@/components/erd/ErdGraph";

describe("ErdGraph zoom controls", () => {
  afterEach(() => vi.restoreAllMocks());

  it("zooms in and out from the current scale", () => {
    let handle: { zoomIn: () => void; zoomOut: () => void } | undefined;
    render(() => (
      <ErdGraph
        nodes={[]}
        edges={[]}
        relations={[]}
        onSelect={vi.fn()}
        onMount={(mounted) => {
          handle = mounted;
        }}
      />
    ));

    const graph = (
      Graph as unknown as {
        last: {
          zoom: ReturnType<typeof vi.fn>;
          zoomTo: ReturnType<typeof vi.fn>;
        };
      }
    ).last;
    graph.zoom.mockReturnValueOnce(0.75).mockReturnValueOnce(1.5);

    handle?.zoomIn();
    handle?.zoomOut();

    expect(graph.zoomTo).toHaveBeenNthCalledWith(1, 1.5);
    expect(graph.zoomTo).toHaveBeenNthCalledWith(2, 0.75);
    expect(graph.zoom).toHaveBeenCalledTimes(2);
  });
});
