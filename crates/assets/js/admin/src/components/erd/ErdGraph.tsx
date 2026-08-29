import { createEffect, onCleanup } from "solid-js";
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

export function layoutErdNodes(nodes: NodeMetadata[]): NodeMetadata[] {
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
  const columns = Math.max(1, Math.ceil(Math.sqrt(Math.max(1, nodes.length))));
  return nodes.map((node, index) => ({
    ...node,
    position: node.position ?? {
      x: (index % columns) * width,
      y: Math.floor(index / columns) * height,
    },
  }));
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
    attrs: { line: { stroke: EDGE_COLOR, strokeWidth: 1 } },
  });
}

export function ErdGraph(props: {
  class?: string;
  nodes: NodeMetadata[];
  edges: EdgeMetadata[];
  relations: { sourceId: string; targetId: string }[];
  selectedId?: string;
  onSelect: (id?: string) => void;
  onMount?: (handle: ErdGraphHandle) => void;
}) {
  let ref: HTMLDivElement | undefined;
  let graph: Graph | undefined;
  const applySelection = () => {
    if (!graph) return;
    const related = new Set<string>(props.selectedId ? [props.selectedId] : []);
    if (props.selectedId)
      for (const relation of props.relations) {
        if (relation.sourceId === props.selectedId)
          related.add(relation.targetId);
        if (relation.targetId === props.selectedId)
          related.add(relation.sourceId);
      }
    graph
      .getNodes()
      .forEach((node) =>
        node.attr(
          "body/stroke",
          props.selectedId && !related.has(node.id)
            ? "var(--muted)"
            : node.id === props.selectedId
              ? "var(--primary)"
              : "var(--border)",
        ),
      );
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
        !!props.selectedId &&
        (source === props.selectedId || target === props.selectedId);
      edge.attr(
        "line/stroke",
        props.selectedId && !connected
          ? "var(--muted)"
          : connected
            ? RELATED_EDGE_COLOR
            : EDGE_COLOR,
      );
      edge.attr("line/strokeWidth", connected ? 2 : 1);
    });
  };
  createEffect(() => {
    const nodes = props.nodes,
      edges = props.edges;
    graph?.dispose();
    const g = (graph = new Graph({
      container: ref,
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
      ...layoutErdNodes(nodes).map((node) => g.createNode(node)),
      ...edges.map((edge) => g.createEdge(edge)),
    ]);
    g.on("node:click", ({ node }) => props.onSelect(node.id));
    g.on("blank:click", () => props.onSelect(undefined));
    const handle: ErdGraphHandle = {
      zoomIn: () => g.zoomTo(g.zoom() * 2),
      zoomOut: () => g.zoomTo(g.zoom() / 2),
      fit: () => g.zoomToFit({ padding: 20 }),
      reset: () => {
        layoutErdNodes(nodes).forEach((node, index) => {
          const position = node.position;
          if (position) g.getNodes()[index]?.position(position.x, position.y);
        });
        g.zoomToFit({ padding: 20 });
      },
      focus: (id) => {
        if (id) {
          const cell = g.getCellById(id);
          if (cell) g.centerCell(cell);
        }
      },
    };
    props.onMount?.(handle);
    if (g.getCells().length) g.zoomToFit({ padding: 20 });
    onCleanup(() => g.dispose());
  });
  createEffect(applySelection);
  return <div ref={ref} class={cn(props.class, "overflow-clip")} />;
}
