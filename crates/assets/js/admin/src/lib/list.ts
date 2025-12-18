import { parse, ExprGroup, Expr, JoinOp, SignOp } from "@/lib/fexpr";
import { showToast } from "@/components/ui/toast";

export type ListArgs = {
  filter: string | undefined | null;
  pageSize: number;
  pageIndex?: number;
  cursor: string | undefined | null;
};

export function buildListSearchParams({
  filter,
  pageSize,
  pageIndex,
  cursor,
}: ListArgs): URLSearchParams {
  const params = new URLSearchParams();

  if (filter) {
    try {
      const filterParams = parseFilter(filter);
      console.debug(`Filter search params: ${filterParams}`);

      for (const [key, value] of filterParams) {
        params.set(key, value);
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

  if (cursor) {
    params.set("cursor", cursor);
  } else {
    if (pageIndex) {
      params.set("offset", `${pageIndex * pageSize}`);
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
      const leftLiteral = child.Left?.Literal ?? "";
      const signOp = child.Op;
      const rightLiteral = child.Right?.Literal ?? "";

      if (rightLiteral === "NULL") {
        // Special case NULL.
        switch (signOp) {
          case undefined:
          case SignOp.Eq:
            filters.push([`${path}[${leftLiteral}][$is]`, "NULL"]);
            break;
          case SignOp.Neq:
            filters.push([`${path}[${leftLiteral}][$is]`, "!NULL"]);
            break;
          default:
            throw Error(`Not supported op: ${signOp}`);
        }
      } else {
        if (signOp !== undefined) {
          filters.push([
            `${path}[${leftLiteral}][${formatOp(signOp)}]`,
            `${rightLiteral}`,
          ]);
        } else {
          filters.push([`${path}[${leftLiteral}]`, `${rightLiteral}`]);
        }
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
      throw Error(`Not supported op: ${op}`);
  }
}
