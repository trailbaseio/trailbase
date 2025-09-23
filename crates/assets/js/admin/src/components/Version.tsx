import { Match, Switch } from "solid-js";
import type { InfoResponse } from "@bindings/InfoResponse";

export function Version(props: { info: InfoResponse | undefined }) {
  // Version tags have the shape <tag>[-<n>-<hash>], where the latter part is
  // missing if it's an exact match. Otherwise, it will contain a reference to
  // the actual commit and how many commits `n` are in between.
  const version = () => props.info?.git_version?.[0] ?? "?";
  const commits_since = () => props.info?.git_version?.[1] ?? 0;

  return (
    <Switch>
      <Match when={commits_since() === 0}>
        {/* We have an exact match, likely a release commit. */}
        <a
          href={`https://github.com/trailbaseio/trailbase/releases/tag/${version()}`}
        >
          {version()}
        </a>
      </Match>

      <Match when={commits_since() > 0}>
        <a
          href={`https://github.com/trailbaseio/trailbase/commit/${props.info?.commit_hash}`}
        >
          {`${version()} (${commits_since()})`}
        </a>
      </Match>
    </Switch>
  );
}
