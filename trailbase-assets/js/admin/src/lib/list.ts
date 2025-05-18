import { parse, ExprGroup, Expr, JoinOp, SignOp } from "@/lib/fexpr";
import { showToast } from "@/components/ui/toast";

export type ListArgs = {
  filter: string | undefined | null;

  pageSize: number;
  pageIndex: number;

  cursor: string | undefined | null;
  prevCursors: string[];
};

export function buildListSearchParams({
  filter,
  pageSize,
  pageIndex,
  cursor,
  prevCursors,
}: ListArgs): URLSearchParams {
  const params = new URLSearchParams();

  if (filter) {
    try {
      const filterParams = parseFilter(filter);
      console.debug(`Filter search prams: ${filterParams}`);

      for (const [filter, value] of filterParams) {
        params.set(filter, value);
      }
    } catch (err) {
      showToast({
        title: "Parse Error",
        description: `${err}`,
        variant: "error",
      });
    }
  }

  params.set("limit", pageSize.toString());

  // Build the next cursor from previous response and update local
  // cursor stack. If we're paging forward we add new cursors, otherwise we're
  // re-using previously seen cursors for consistency. We reset if we go back
  // to the start.
  if (pageIndex === 0) {
    prevCursors.length = 0;
  } else {
    const index = pageIndex - 1;
    if (index < prevCursors.length) {
      // Already known page
      params.set("cursor", prevCursors[index]);
    } else {
      // New page case: use cursor from previous response or fall back to more
      // expensive and inconsistent offset-based pagination.
      if (cursor) {
        prevCursors.push(cursor);
        params.set("cursor", cursor);
      } else {
        params.set("offset", `${pageIndex * pageSize}`);
      }
    }
  }

  return params;
}

export function parseFilter(expr: string): [string, string][] {
  if (expr === "") {
    return [];
  }
  const ast: ExprGroup[] = parse(expr);

  const filters: [string, string][] = [];
  function traverseExpr(path: string, child: Expr | ExprGroup | ExprGroup[]) {
    if (child instanceof Expr) {
      const signOp = child.Op;
      if (signOp !== undefined) {
        filters.push([
          `${path}[${child.Left?.Literal}][${formatOp(signOp)}]`,
          child.Right?.Literal?.toString() ?? "",
        ]);
      } else {
        filters.push([
          `${path}[${child.Left?.Literal}]`,
          child.Right?.Literal?.toString() ?? "",
        ]);
      }
    } else if (child instanceof ExprGroup) {
      traverseExpr(path, child.Item);
    } else if (child instanceof Array) {
      if (child.length === 0) {
        return;
      } else if (child.length === 1) {
        traverseExpr(path, child[0].Item);
      } else {
        // NOTE: the first one is always "&&" :/. Thus grab the second and
        // assert that all match within the group.
        const join: JoinOp = child[1].Join!;
        const op = join == "&&" ? "$and" : "$or";

        for (const [i, c] of child.entries()) {
          if (i > 0 && c.Join !== join) {
            throw Error("No implicit &&/|| precedence");
          }
          traverseExpr(`${path}[${op}][${i}]`, c);
        }
      }
    } else {
      throw Error("unreachable");
    }
  }

  traverseExpr("filter", ast);

  return filters;
}

function formatOp(op: SignOp): string {
  switch (op) {
    case SignOp.Eq:
      return "$eq";
    case SignOp.Neq:
      return "$ne";
    case SignOp.Like:
      return "$like";
    case SignOp.Lt:
      return "$lt";
    case SignOp.Lte:
      return "$lte";
    case SignOp.Gt:
      return "$gt";
    case SignOp.Gte:
      return "$gte";
    case SignOp.Nlike:
    case SignOp.AnyEq:
    case SignOp.AnyNeq:
    case SignOp.AnyLike:
    case SignOp.AnyNlike:
    case SignOp.AnyLt:
    case SignOp.AnyLte:
    case SignOp.AnyGt:
    case SignOp.AnyGte:
      throw Error("Not supported");
  }
}
