# ERD Workspace Refresh Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Turn the ERD into a search-first, data-dense schema workspace with entity focus, useful filters, reliable canvas controls, and polished states.

**Architecture:** Keep `ErdPage` responsible for declarative UI state and schema fetching, and keep `ErdGraph` as the narrow imperative X6 boundary. Add pure schema-model, search, filtering, layout, and neighborhood helpers so the non-DOM behavior is covered without mocking X6.

**Tech Stack:** SolidJS, TypeScript, Kobalte primitives, Tailwind CSS, AntV X6 3.1.8, TanStack Solid Query, Vitest, Solid Testing Library.

---

## Constraints

- Preserve `/_/admin/erd`, `createTableSchemaQuery`, schema bindings, hidden-table behavior, `_user` handling, theme switching, node dragging, panning, and wheel zoom.
- Add no dependencies, backend endpoints, persisted layouts, schema editing, minimap, inspector, or explorer sidebar.
- Reuse `Header`, `Button`, `Toggle`, `Badge`, `Callout`, `Spinner`, and existing theme tokens.
- Use native CSS variables inside X6 SVG attributes where supported rather than duplicating palette constants.

### Task 1: Extract and test the ERD workspace model

**Files:**
- Create: `crates/assets/js/admin/tests/erd-workspace.test.ts`
- Modify: `crates/assets/js/admin/src/components/erd/ErdPage.tsx:25-157`

**Step 1: Write fixture helpers and failing model tests**

Create minimal `column`, `table`, `view`, and `schema` fixture builders in the test file. Keep every required generated binding field explicit:

```ts
function column(name: string, options: ColumnOption[] = []): Column {
  return {
    name,
    type_name: "TEXT",
    data_type: "Text",
    affinity_type: "Text",
    options,
  };
}

function table(name: string, columns: Column[]): Table {
  return {
    name: { name, database_schema: "main" },
    strict: true,
    columns,
    foreign_keys: [],
    unique: [],
    checks: [],
    virtual_table: false,
    temporary: false,
  };
}
```

Add tests that assert:

1. `buildErdModel` returns table/view entities and their counts.
2. Hidden internal tables remain excluded while `main._user` remains included.
3. Hiding views removes view nodes and any edge targeting or originating from them.
4. A foreign-key column creates one relation with stable `sourceId` and `targetId`.
5. `relatedEntityIds(model.relations, id)` returns the selected entity and direct neighbors only.
6. `searchErdEntities` matches case-insensitively by qualified name and returns all entities for an empty query.

Use a foreign-key option such as:

```ts
{
  ForeignKey: {
    foreign_table: "users",
    referred_columns: ["id"],
    on_delete: null,
    on_update: null,
  },
}
```

**Step 2: Run the focused test and verify failure**

Run:

```bash
pnpm --dir crates/assets/js/admin vitest run tests/erd-workspace.test.ts
```

Expected: FAIL because `buildErdModel`, `relatedEntityIds`, and `searchErdEntities` are not exported.

**Step 3: Implement the smallest pure model**

In `ErdPage.tsx`, introduce:

```ts
export type ErdEntityType = "table" | "view";

export type ErdEntity = {
  id: string;
  name: string;
  type: ErdEntityType;
};

export type ErdRelation = {
  sourceId: string;
  targetId: string;
};

export type ErdModel = {
  entities: ErdEntity[];
  nodes: NodeMetadata[];
  edges: EdgeMetadata[];
  relations: ErdRelation[];
  tableCount: number;
  viewCount: number;
};

export type ErdVisibility = {
  tables: boolean;
  views: boolean;
};
```

Export these pure helpers:

```ts
buildErdModel(
  schema: ListSchemasResponse,
  visibility: ErdVisibility,
): ErdModel

searchErdEntities(entities: ErdEntity[], query: string): ErdEntity[]

relatedEntityIds(relations: ErdRelation[], selectedId?: string): Set<string>
```

Move the current table/view traversal into `buildErdModel`. Filter hidden entities and table/view visibility before graph construction. Build a visible-ID `Set` and retain only edges whose source and target are both visible. Keep `buildErNode` private and preserve its port lookup behavior.

Use `prettyFormatQualifiedName` as the stable entity/node ID. Avoid a new model file; these helpers only serve this screen.

**Step 4: Run the focused test and TypeScript**

Run:

```bash
pnpm --dir crates/assets/js/admin vitest run tests/erd-workspace.test.ts
pnpm --dir crates/assets/js/admin tsc --noEmit --skipLibCheck
```

Expected: all ERD model tests pass and TypeScript exits 0.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/erd/ErdPage.tsx crates/assets/js/admin/tests/erd-workspace.test.ts
git commit -m "refactor(admin): extract erd workspace model"
```

### Task 2: Add the tested search and toolbar UI

**Files:**
- Create: `crates/assets/js/admin/tests/erd-workspace-ui.test.tsx`
- Modify: `crates/assets/js/admin/src/components/erd/ErdPage.tsx:158-257`

**Step 1: Write failing toolbar tests**

Export an `ErdToolbar` component from `ErdPage.tsx` with direct props rather than a shared workspace abstraction:

```ts
{
  entities: ErdEntity[];
  showTables: boolean;
  showViews: boolean;
  selectedId?: string;
  onShowTablesChange(value: boolean): void;
  onShowViewsChange(value: boolean): void;
  onSelect(id?: string): void;
  onZoomIn(): void;
  onZoomOut(): void;
  onFit(): void;
  onReset(): void;
}
```

In `erd-workspace-ui.test.tsx`, render two entities and assert:

- Search has accessible name `Search entities`.
- Entering `post` shows only the matching entity.
- ArrowDown then Enter calls `onSelect("main.post")`.
- Escape closes results; when results are closed it calls `onSelect(undefined)`.
- Tables and Views toggles reflect pressed state and invoke their callbacks.
- Buttons named `Zoom in`, `Zoom out`, `Fit view`, and `Reset layout` invoke callbacks.

Use `fireEvent.input`, `fireEvent.keyDown`, and role-based queries.

**Step 2: Run the focused test and verify failure**

```bash
pnpm --dir crates/assets/js/admin vitest run tests/erd-workspace-ui.test.tsx
```

Expected: FAIL because `ErdToolbar` is not exported.

**Step 3: Implement the toolbar**

Use a native search input and a small ARIA listbox; do not add a combobox dependency. Structure the search control as:

```tsx
<input
  type="search"
  role="combobox"
  aria-label="Search entities"
  aria-autocomplete="list"
  aria-expanded={open()}
  aria-controls="erd-search-results"
/>
```

Render results in `#erd-search-results` with `role="listbox"` and result buttons with `role="option"`. Each result shows the qualified entity name and a Table/View badge. Track only query, open state, and active index locally. Clamp the active index whenever results change.

Use existing `Toggle` components for Tables and Views and existing `Button` variants for graph commands. Add icons already available from `solid-icons/tb`; pair icon-only controls with `aria-label` and `title`.

Place the toolbar in a bordered `bg-card` row beneath `Header`. Search is `w-full sm:max-w-sm`; action groups wrap rather than overflow.

**Step 4: Run focused tests**

```bash
pnpm --dir crates/assets/js/admin vitest run tests/erd-workspace-ui.test.tsx tests/erd-workspace.test.ts
```

Expected: both files pass.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/erd/ErdPage.tsx crates/assets/js/admin/tests/erd-workspace-ui.test.tsx
git commit -m "feat(admin): add erd search toolbar"
```

### Task 3: Improve graph cards, layout, and focus behavior

**Files:**
- Modify: `crates/assets/js/admin/src/components/erd/ErdGraph.tsx:1-270`
- Modify: `crates/assets/js/admin/src/components/erd/ErdPage.tsx`
- Modify: `crates/assets/js/admin/tests/erd-workspace.test.ts`

**Step 1: Add failing layout and focus tests**

Add pure tests for:

```ts
layoutErdNodes(nodes, aspect)
focusedErdIds(relations, selectedId)
```

Verify that layout returns new node metadata rather than mutating input, assigns deterministic grid positions, and handles zero nodes without `NaN` or division by zero. Verify that focus contains the selected node plus direct neighbors and excludes second-degree nodes.

**Step 2: Run tests and verify failure**

```bash
pnpm --dir crates/assets/js/admin vitest run tests/erd-workspace.test.ts
```

Expected: FAIL because the layout helper is not exported.

**Step 3: Refactor the X6 boundary minimally**

In `ErdGraph.tsx`:

- Export `layoutErdNodes(nodes, aspect)` and use it for initial and reset positions.
- Stop mutating incoming `NodeMetadata` objects.
- Guard the empty-node case before calculating rows or zoom rectangles.
- Replace hard-coded black/white card colors with `var(--card)`, `var(--card-foreground)`, `var(--border)`, `var(--muted-foreground)`, and `var(--primary)` SVG values.
- Use a neutral card body and compact primary-colored selection stroke instead of a solid blue header on every node.
- Add a right-aligned `typeLabel` selector and set it to `TABLE` or `VIEW` from node attributes.
- Prefix foreign-key and primary-key column labels with `FK ·` and `PK ·`; retain `?` nullability on types.
- Keep edges at 1.5–2px, muted by default.

Expose a screen-specific handle:

```ts
export type ErdGraphHandle = {
  zoomIn(): void;
  zoomOut(): void;
  fit(): void;
  reset(): void;
  focus(id: string): void;
};
```

Update graph props:

```ts
{
  nodes: NodeMetadata[];
  edges: EdgeMetadata[];
  relations: ErdRelation[];
  selectedId?: string;
  onSelect(id?: string): void;
  onMount?(handle: ErdGraphHandle): void;
}
```

Wire X6 `node:click` to `onSelect(node.id)` and `blank:click` to `onSelect(undefined)`. Use the installed X6 3.1.8 APIs verified in local types: `zoomTo`, `zoomToFit`, `centerCell`, `getCellById`, `node.attr`, and `edge.attr`.

Keep graph construction and selection styling in separate Solid effects so changing selection does not recreate the graph. For focus, set unrelated node selector opacity and unrelated edge line opacity lower; emphasize connected edge stroke and restore every attribute when selection clears.

**Step 4: Wire the toolbar handle**

In `SchemaErdGraph`, keep one `ErdGraphHandle | undefined`. Route toolbar callbacks directly to it. Search selection should update `selectedId` and call `handle.focus(id)`. Reset should clear selection, restore deterministic positions, and fit visible nodes.

Add a visually hidden `aria-live="polite"` status containing either `No entity focused` or `<name> focused, <n> direct relationships`.

**Step 5: Run focused tests and TypeScript**

```bash
pnpm --dir crates/assets/js/admin vitest run tests/erd-workspace.test.ts tests/erd-workspace-ui.test.tsx
pnpm --dir crates/assets/js/admin tsc --noEmit --skipLibCheck
```

Expected: focused tests pass and TypeScript exits 0.

**Step 6: Commit**

```bash
git add crates/assets/js/admin/src/components/erd/ErdGraph.tsx crates/assets/js/admin/src/components/erd/ErdPage.tsx crates/assets/js/admin/tests/erd-workspace.test.ts
git commit -m "feat(admin): add erd relationship focus"
```

### Task 4: Integrate page states and responsive workspace layout

**Files:**
- Modify: `crates/assets/js/admin/src/components/erd/ErdPage.tsx`
- Modify: `crates/assets/js/admin/tests/erd-workspace-ui.test.tsx`

**Step 1: Add failing page-state tests**

Mock `createTableSchemaQuery` and render `ErdPage` for each query state. Assert:

- Pending state shows `Loading schema` and retains the ERD heading.
- Error state shows `Unable to load schema` and a `Retry` button that calls `refetch`.
- Empty schema shows `No schema entities`.
- Disabling both entity toggles shows `No entities match these filters` and `Show all entities`.
- A populated schema shows table/view/relationship counts in the header description.

Keep the query mock local to this test file; do not add a production-only wrapper component just for tests.

**Step 2: Run the page-state tests and verify failure**

```bash
pnpm --dir crates/assets/js/admin vitest run tests/erd-workspace-ui.test.tsx
```

Expected: new state assertions fail against the current raw error and missing loading/empty states.

**Step 3: Implement page states**

Change the page title to `ERD`. Use the Header description for concise counts such as:

```text
12 tables · 3 views · 8 relationships
```

Keep the workspace structure mounted while loading. Render:

- `Spinner` plus `Loading schema` for pending state.
- `Callout variant="error"` with Retry for errors.
- A centered empty treatment for no schema entities.
- A filter-empty treatment with `Show all entities` that enables both toggles.

Use `min-h-0 flex-1` on the canvas wrapper so X6 receives a real bounded height. Let the toolbar use `flex-wrap`; keep search full-width below the small breakpoint and allow the graph to consume the rest of the viewport.

When filtering removes the selected entity, clear selection. When filtering changes visible nodes, call fit after X6 mounts the replacement cells.

**Step 4: Run focused tests, format, and lint touched files**

```bash
pnpm --dir crates/assets/js/admin vitest run tests/erd-workspace.test.ts tests/erd-workspace-ui.test.tsx
pnpm --dir crates/assets/js/admin prettier -w src/components/erd/ErdPage.tsx src/components/erd/ErdGraph.tsx tests/erd-workspace.test.ts tests/erd-workspace-ui.test.tsx
pnpm --dir crates/assets/js/admin eslint src/components/erd/ErdPage.tsx src/components/erd/ErdGraph.tsx tests/erd-workspace.test.ts tests/erd-workspace-ui.test.tsx
```

Expected: focused tests pass and targeted lint has no errors.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/erd/ErdPage.tsx crates/assets/js/admin/src/components/erd/ErdGraph.tsx crates/assets/js/admin/tests/erd-workspace-ui.test.tsx
git commit -m "feat(admin): polish erd workspace states"
```

### Task 5: Browser acceptance and final verification

**Files:**
- Modify only files above if acceptance exposes a defect.

**Step 1: Run the complete automated suite**

```bash
pnpm --dir crates/assets/js/admin check:format
pnpm --dir crates/assets/js/admin check
pnpm --dir crates/assets/js/admin build
git diff --check
```

Expected: formatting passes, TypeScript and tests pass, ESLint has no new errors, production build succeeds, and diff check is clean. Existing unrelated warnings may remain.

**Step 2: Validate desktop behavior in Chrome**

Open `http://localhost:3000/_/admin/erd` and verify:

1. Search `post`, select the result, and confirm the canvas centers it.
2. Confirm direct relationships remain prominent and unrelated entities dim.
3. Press Escape and confirm full emphasis returns.
4. Click a node and then blank canvas to verify selection and clearing.
5. Disable Views, then Tables; confirm no dangling edges and the filter-empty recovery action.
6. Verify Zoom in, Zoom out, Fit view, and Reset layout.
7. Drag a node, reset layout, and confirm its generated position is restored.
8. Switch light/dark themes and inspect card, text, grid, selection, and relationship contrast.
9. Check the accessibility tree for named search, toggles, controls, listbox options, and live status.

**Step 3: Validate responsive behavior**

At approximately 390×844:

- Search occupies the available first row.
- Filters and graph controls wrap without clipping.
- Search results overlay rather than resize the canvas.
- Every action remains reachable with a touch-sized target.
- Pan and zoom remain usable.

Save representative desktop, dark, and mobile screenshots under `/tmp`; do not commit screenshots.

**Step 4: Fix only acceptance defects and rerun affected checks**

For each issue, add or strengthen the smallest focused regression test before changing production code. Rerun the focused test and then the complete verification commands from Step 1.

**Step 5: Final commit if acceptance required changes**

```bash
git add crates/assets/js/admin/src/components/erd crates/assets/js/admin/tests/erd-workspace*.ts*
git commit -m "fix(admin): refine erd workspace interactions"
```

Do not push until explicitly requested.
