import { For } from "solid-js";

export function Tag(props: { tag: string }) {
  const style =
    "text-[16px] border-pacamara-secondary border leading-none rounded-full flex items-center h-[34px] px-2 text-pacamara-secondary";
  return (
    <div class="flex items-center gap-2">
      <For each={props.tag.split(",").map((t) => t.trim())}>
        {(tag) => <span class={style}>{tag}</span>}
      </For>
    </div>
  );
}
