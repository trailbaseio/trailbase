import { onMount } from "solid-js";
import { Graph, Cell, Shape, Edge, NodeMetadata, EdgeMetadata } from "@antv/x6";

import { cn } from "@/lib/utils";

export const ER_NODE_NAME = "er-rect";
export const LINE_HEIGHT = 24;
export const NODE_WIDTH = 250;

const ACCENT_600 = "#0073aa";
const GRAY_100 = "#f3f7f9";
export const EDGE_COLOR = "#A2B1C3";

(function setupGraph() {
  const ER_PORT_POSITION_NAME = "erPortPosition";

  Graph.registerPortLayout(
    ER_PORT_POSITION_NAME,
    (portsPositionArgs) => {
      return portsPositionArgs.map((_, index) => {
        return {
          position: {
            x: 0,
            y: (index + 1) * LINE_HEIGHT,
          },
          angle: 0,
        };
      });
    },
    true,
  );

  Graph.registerNode(
    ER_NODE_NAME,
    {
      inherit: "rect",
      markup: [
        {
          tagName: "rect",
          selector: "body",
        },
        {
          tagName: "text",
          selector: "label",
        },
      ],
      attrs: {
        rect: {
          strokeWidth: 1,
          stroke: ACCENT_600,
          fill: ACCENT_600,
        },
        label: {
          fontWeight: "bold",
          fill: "white",
          fontSize: 12,
        },
      },
      ports: {
        groups: {
          list: {
            markup: [
              {
                tagName: "rect",
                selector: "portBody",
              },
              {
                tagName: "text",
                selector: "portNameLabel",
              },
              {
                tagName: "text",
                selector: "portTypeLabel",
              },
            ],
            attrs: {
              portBody: {
                width: NODE_WIDTH,
                height: LINE_HEIGHT,
                strokeWidth: 1,
                stroke: ACCENT_600,
                fill: GRAY_100,
                magnet: true,
              },
              portNameLabel: {
                ref: "portBody",
                refX: 6,
                refY: 6,
                fontSize: 10,
              },
              portTypeLabel: {
                ref: "portBody",
                refX: 95,
                refY: 6,
                fontSize: 10,
              },
            },
            position: ER_PORT_POSITION_NAME,
          },
        },
      },
    },
    true,
  );
})();

function createEdge(): Edge {
  return new Shape.Edge({
    attrs: {
      line: {
        stroke: EDGE_COLOR,
        strokeWidth: 2,
      },
    },
  });
}

export function ErdGraph(props: {
  class?: string;
  nodes: NodeMetadata[];
  edges: EdgeMetadata[];
  onMount?: (graph: Graph) => void;
}) {
  let ref: HTMLDivElement | undefined;

  onMount(() => {
    const graph = new Graph({
      container: ref,
      grid: {
        visible: true,
      },
      autoResize: true,
      interacting: {
        edgeLabelMovable: false,
        magnetConnectable: false,
      },
      connecting: {
        connector: "rounded",
        router: {
          name: "er",
          args: {
            offset: 25,
            direction: "H",
          },
        },
        createEdge,
      },
      panning: {
        enabled: true,
      },
      mousewheel: {
        enabled: true,
        // modifiers: ['ctrl', 'meta'],
        minScale: 0.5,
        maxScale: 2,
      },
    });

    // Implement our own simple grid layout since @antv/layout seems to be out of sync:
    //
    // v0.3.25 results in "layout is not a function": https://github.com/antvis/X6/issues/4441
    // v1.2 has completely in-compatible APIs. They'll probably need to overhaul x6 first.
    const maxHeight = props.nodes.reduce((acc, node) => {
      const ports = node.ports;
      let numPorts = 0;
      if (ports instanceof Array) {
        numPorts = ports.length;
      } else if (ports !== undefined) {
        numPorts = 1;
      }
      return Math.max(acc, (numPorts + 1) * LINE_HEIGHT);
    }, 0);

    const width = NODE_WIDTH + 20;
    const height = maxHeight + 10;

    const nodeAspect = width / height;
    const screenAspect = window.innerWidth / window.innerHeight;
    const columns = Math.floor(
      Math.sqrt(props.nodes.length) * (screenAspect / nodeAspect),
    );

    const cells: Cell[] = [
      ...props.nodes.map((n, index) => {
        // Scatter nodes on a grid if no explicit position is already set.
        if (n.position === undefined) {
          const col = index % columns;
          const row = Math.floor(index / columns);

          n.position = {
            x: col * width,
            y: row * height,
          };
        }

        return graph.createNode(n);
      }),
      ...props.edges.map((e) => graph.createEdge(e)),
    ];

    graph.resetCells(cells);
    graph.zoomToFit({ padding: 20, maxScale: 1, minScale: 0.1 });

    props.onMount?.(graph);
  });

  return <div ref={ref} class={cn(props.class, "overflow-clip")} />;
}
