import { join } from "node:path";
import { readFileSync } from "node:fs";
import { cwd } from "node:process";

const repo = "https://github.com/trailbaseio/trailbase";

/// Takes a repo `path`, finds the `match` and constructs a github link with
/// the line number for the match.
export function githubCodeReference(args: {
  path: string;
  match: string;
}): string {
  const pwd = cwd();
  const root = join(pwd, "..");
  const path = join(root, args.path);

  const buffer = readFileSync(path);

  const matches: number[] = buffer
    .toString()
    .split("\n")
    .reduce((prev: number[], curr: string, index: number, _) => {
      if (curr.includes(args.match)) {
        const lineNumber = index + 1;
        prev.push(lineNumber);
      }
      return prev;
    }, []);

  switch (matches.length) {
    case 0:
      throw new Error(`Not match for '${args.match}' in: ${args.path}`);
    case 1:
      return join(repo, `/blob/main/${args.path}#L${matches[0]}`);
    default:
      throw new Error(
        `Ambiguous matches for '${args.match}' at lines: ${matches} in: ${args.path}`,
      );
  }
}
