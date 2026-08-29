import { createEffect, onCleanup, untrack } from "solid-js";
import { Graph, Shape, Edge, NodeMetadata, EdgeMetadata } from "@antv/x6";
import { cn } from "@/lib/utils";
import type { ResolvedTheme } from "@/lib/theme";

export const LINE_HEIGHT = 24;
export const NODE_WIDTH = 250;
const EDGE_COLOR = "var(--border)";
const RELATED_EDGE_COLOR = "var(--primary)";

type Theme = { fill: string; accent: string; edge: string; text: string };
const lightTheme: Theme = {
  fill: "var(--card)",
  accent: "var(--border)",
  edge: EDGE_COLOR,
  text: "var(--card-foreground)",
};
const darkTheme = lightTheme;

export function nodeName(theme: ResolvedTheme): string {
  return theme === "dark" ? "dark:er-rect" : "light:er-rect";
}
export function erdTheme(dark: boolean): Theme {
  return dark ? darkTheme : lightTheme;
}

function setupGraph() {
  Graph.registerPortLayout(
    "erPortPosition",
    (ports) =>
      ports.map((_, index) => ({
        position: { x: 0, y: (index + 1) * LINE_HEIGHT },
        angle: 0,
      })),
    true,
  );
  for (const themeName of ["light", "dark"] as ResolvedTheme[]) {
    Graph.registerNode(
      nodeName(themeName),
      {
        inherit: "rect",
        markup: [
          { tagName: "rect", selector: "body" },
          { tagName: "text", selector: "label" },
          { tagName: "text", selector: "typeLabel" },
        ],
        attrs: {
          body: {
            strokeWidth: 2,
            stroke: "var(--border)",
            fill: "var(--card)",
          },
          label: {
            fontWeight: "bold",
            fill: "var(--card-foreground)",
            fontSize: 12,
            refX: 8,
            refY: 12,
            textAnchor: "start",
          },
          typeLabel: {
            fill: "var(--muted-foreground)",
            fontSize: 10,
            refX: NODE_WIDTH - 8,
            refY: 12,
            textAnchor: "end",
          },
        },
        ports: {
          groups: {
            list: {
              markup: [
                { tagName: "rect", selector: "portBody" },
                { tagName: "text", selector: "portNameLabel" },
                { tagName: "text", selector: "portTypeLabel" },
              ],
              attrs: {
                portBody: {
                  width: NODE_WIDTH,
                  height: LINE_HEIGHT,
                  strokeWidth: 1,
                  stroke: "var(--border)",
                  fill: "var(--card)",
                },
                portNameLabel: {
                  ref: "portBody",
                  refX: 8,
                  refY: 6,
                  fontSize: 10,
                  fill: "var(--card-foreground)",
                },
                portTypeLabel: {
                  ref: "portBody",
                  refX: 180,
                  refY: 6,
                  fontSize: 10,
                  fill: "var(--muted-foreground)",
                },
              },
              position: "erPortPosition",
            },
          },
        },
      },
      true,
    );
  }
}
setupGraph();

export function layoutErdNodes(
  nodes: NodeMetadata[],
  aspect: number,
): NodeMetadata[] {
  if (nodes.length === 0) return [];

  const width = NODE_WIDTH + 20;
  const height = Math.max(
    34,
    ...nodes.map(
      (node) =>
        ((node.ports instanceof Array ? node.ports.length : 0) + 1) *
          LINE_HEIGHT +
        10,
    ),
  );
  const safeAspect = Number.isFinite(aspect) && aspect > 0 ? aspect : 1;
  const columns = Math.max(
    1,
    Math.ceil(Math.sqrt((safeAspect * nodes.length * height) / width)),
  );

  return nodes.map((node, index) => ({
    ...node,
    position: node.position ?? {
      x: (index % columns) * width,
      y: Math.floor(index / columns) * height,
    },
  }));
}

export function focusedErdIds(
  relations: { sourceId: string; targetId: string }[],
  selectedId?: string,
): Set<string> {
  const focused = new Set<string>();
  if (!selectedId) return focused;

  focused.add(selectedId);
  for (const relation of relations) {
    if (relation.sourceId === selectedId) focused.add(relation.targetId);
    if (relation.targetId === selectedId) focused.add(relation.sourceId);
  }
  return focused;
}

type ErdOpacityNode = {
  attr: (attributes: Record<string, { opacity: number }>) => unknown;
  getPorts: () => { id?: string }[];
  setPortProp: (id: string, value: Record<string, unknown>) => unknown;
};

export function setErdNodeOpacity(node: ErdOpacityNode, opacity: number): void {
  node.attr({
    body: { opacity },
    label: { opacity },
    typeLabel: { opacity },
  });
  for (const port of node.getPorts()) {
    if (!port.id) continue;
    node.setPortProp(port.id, {
      attrs: {
        portBody: { opacity },
        portNameLabel: { opacity },
        portTypeLabel: { opacity },
      },
    });
  }
}

export type ErdGraphHandle = {
  zoomIn: () => void;
  zoomOut: () => void;
  fit: () => void;
  reset: () => void;
  focus: (id?: string) => void;
};

function createEdge(): Edge {
  return new Shape.Edge({
    attrs: {
      line: { stroke: EDGE_COLOR, strokeWidth: 1.5, opacity: 0.65 },
    },
  });
}

export function ErdGraph(props: {
  class?: string;
  nodes: NodeMetadata[];
  edges: EdgeMetadata[];
  relations: { sourceId: string; targetId: string }[];
  selectedId?: string;
  onSelect: (id?: string) => void;
  onMount?: (handle?: ErdGraphHandle) => void;
}) {
  let ref: HTMLDivElement | undefined;
  let graph: Graph | undefined;
  const graphAspect = () => {
    const width = ref?.clientWidth || window.innerWidth;
    const height = ref?.clientHeight || window.innerHeight;
    return width > 0 && height > 0 ? width / height : 1;
  };
  const applySelection = () => {
    if (!graph) return;

    const focused = focusedErdIds(props.relations, props.selectedId);
    const hasSelection = props.selectedId !== undefined;
    graph.getNodes().forEach((node) => {
      const opacity = hasSelection && !focused.has(node.id) ? 0.28 : 1;
      setErdNodeOpacity(node, opacity);
      node.attr(
        "body/stroke",
        node.id === props.selectedId ? "var(--primary)" : "var(--border)",
      );
    });
    graph.getEdges().forEach((edge) => {
      const source =
        typeof edge.getSourceCellId === "function"
          ? edge.getSourceCellId()
          : undefined;
      const target =
        typeof edge.getTargetCellId === "function"
          ? edge.getTargetCellId()
          : undefined;
      const connected =
        hasSelection &&
        (source === props.selectedId || target === props.selectedId);
      edge.attr({
        line: {
          opacity: hasSelection ? (connected ? 1 : 0.12) : 0.65,
          stroke: connected ? RELATED_EDGE_COLOR : EDGE_COLOR,
          strokeWidth: connected ? 2 : 1.5,
        },
      });
    });
  };
  createEffect(() => {
    const nodes = props.nodes,
      edges = props.edges,
      onSelect = props.onSelect,
      onMount = props.onMount;
    graph?.dispose();
    const g = (graph = new Graph({
      container: ref,
      grid: { visible: true },
      autoResize: true,
      interacting: { edgeLabelMovable: false, magnetConnectable: false },
      connecting: {
        connector: "rounded",
        router: { name: "er", args: { offset: 25, direction: "H" } },
        createEdge,
      },
      panning: { enabled: true },
      mousewheel: { enabled: true, minScale: 0.5, maxScale: 2 },
    }));
    g.resetCells([
      ...layoutErdNodes(nodes, graphAspect()).map((node) => g.createNode(node)),
      ...edges.map((edge) => g.createEdge(edge)),
    ]);
    untrack(applySelection);
    g.on("node:click", ({ node }) => onSelect(node.id));
    g.on("blank:click", () => onSelect(undefined));
    const handle: ErdGraphHandle = {
      zoomIn: () => g.zoomTo(g.zoom() * 2),
      zoomOut: () => g.zoomTo(g.zoom() / 2),
      fit: () => g.zoomToFit({ padding: 20 }),
      reset: () => {
        layoutErdNodes(nodes, graphAspect()).forEach((node, index) => {
          const position = node.position;
          if (position) g.getNodes()[index]?.position(position.x, position.y);
        });
        g.zoomToFit({ padding: 20 });
      },
      focus: (id) => {
        if (id) {
          const cell = g.getCellById(id);
          if (cell) {
            g.zoomTo(1);
            g.centerCell(cell);
          }
        }
      },
    };
    onMount?.(handle);
    if (g.getCells().length) g.zoomToFit({ padding: 20 });
    onCleanup(() => {
      if (graph === g) graph = undefined;
      onMount?.(undefined);
      g.dispose();
    });
  });
  createEffect(applySelection);
  return <div ref={ref} class={cn(props.class, "overflow-clip")} />;
}
