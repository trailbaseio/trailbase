import { isServer } from "solid-js/web";
import { Show } from "solid-js";

export function ScreenDimensions() {
  return (
    <Show when={!isServer}>
      <span>
        {window.innerWidth}x{window.innerHeight} (WxH)
      </span>
    </Show>
  );
}
