import { Match, Switch } from "solid-js";
import type { InfoResponse } from "@bindings/InfoResponse";

export function Version(props: { info: InfoResponse | undefined }) {
  // Version tags have the shape <tag>[-<n>-<hash>], where the latter part is
  // missing if it's an exact match. Otherwise, it will contain a reference to
  // the actual commit and how many commits `n` are in between.
  const version = () => props.info?.git_version;
  const commits_since = () => version()?.offset ?? 0;

  return (
    <Switch>
      <Match when={commits_since() > 0}>
        <a
          href={`https://github.com/trailbaseio/trailbase/commit/${props.info?.commit_hash}`}
        >
          {`${version()} (${commits_since()})`}
        </a>
      </Match>

      <Match when={version()}>
        {(v) => {
          // We have an exact match, likely a release commit.
          return (
            <a
              href={`https://github.com/trailbaseio/trailbase/releases/tag/${version()}`}
            >
              {v().tag}
            </a>
          );
        }}
      </Match>

      <Match when={true}>{props.info?.commit_hash?.substring(0, 10)}</Match>
    </Switch>
  );
}
