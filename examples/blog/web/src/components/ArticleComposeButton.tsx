import { Show } from "solid-js";
import { useStore } from "@nanostores/solid";
import { TbPencilPlus } from "solid-icons/tb";

import { $profile } from "@/lib/profile";

export function ArticleComposeButton() {
  const profile = useStore($profile);

  return (
    <Show when={profile()?.profile?.is_editor ?? false} fallback={<></>}>
      <a href="/compose">
        <TbPencilPlus class="inline-block size-6 rounded-full bg-pacamara-secondary p-1 dark:text-white" />
      </a>
    </Show>
  );
}
