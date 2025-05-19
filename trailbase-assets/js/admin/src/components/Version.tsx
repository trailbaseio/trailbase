import { createMemo, Match, Switch } from "solid-js";
import type { InfoResponse } from "@bindings/InfoResponse";

export function Version(props: { info: InfoResponse | undefined }) {
  // Version tags have the shape <tag>[-<n>-<hash>], where the latter part is
  // missing if it's an exact match. Otherwise, it will contain a reference to
  // the actual commit and how many commits `n` are in between.
  const fragments = createMemo(() => props.info?.version_tag?.split("-") ?? []);
  return (
    <Switch>
      <Match when={fragments().length === 1}>
        {/* We have an exact match, likely a release commit. */}
        <a
          href={`https://github.com/trailbaseio/trailbase/releases/tag/${fragments()[0]}`}
        >
          {fragments()[0]}
        </a>
      </Match>

      <Match when={fragments().length > 1}>
        <a
          href={`https://github.com/trailbaseio/trailbase/commit/${props.info?.commit_hash}`}
        >
          {`${fragments()[0]} (${fragments()[1]})`}
        </a>
      </Match>
    </Switch>
  );
}
