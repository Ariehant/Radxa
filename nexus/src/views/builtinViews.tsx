import { registerFlowView } from "./registry";
import { Canvas } from "./FlowEditor/Canvas";
import { TableView } from "./TableView/TableView";
import { HybridView } from "./HybridView/HybridView";

registerFlowView({
  id: "canvas",
  label: "Canvas",
  order: 10,
  palette: true,
  inspector: true,
  render: (ctx) => <Canvas renderActions={ctx.renderActions} renderStatus={ctx.renderStatus} />,
});

registerFlowView({
  id: "table",
  label: "Table",
  order: 20,
  palette: false,
  inspector: true,
  render: () => <TableView />,
});

registerFlowView({
  id: "hybrid",
  label: "Hybrid",
  order: 30,
  palette: true,
  inspector: false,
  render: (ctx) => <HybridView left={<Canvas renderActions={ctx.renderActions} renderStatus={ctx.renderStatus} />} right={<TableView />} />,
});
