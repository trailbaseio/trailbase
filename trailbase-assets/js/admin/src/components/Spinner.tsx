import { cn } from "@/lib/utils";

export function Spinner(props: { class?: string; size?: number }) {
  return (
    <svg
      class={cn("animate-spin", props.class)}
      style={{
        width: `${props.size ?? 32}px`,
        height: `${props.size ?? 32}px`,
      }}
      xmlns="http://www.w3.org/2000/svg"
      fill="none"
      viewBox="0 0 24 24"
    >
      <title>Loading</title>
      <circle
        class="opacity-25"
        cx="12"
        cy="12"
        r="10"
        stroke="currentColor"
        stroke-width="4"
      />
      <path
        class="opacity-75"
        fill="currentColor"
        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
      />
    </svg>
  );
}
