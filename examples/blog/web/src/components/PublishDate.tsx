export function PublishDate(props: { date: number }) {
  return (
    <span class="font-pacamara-space text-[16px] text-pacamara-primary/50 transition-all duration-300 dark:text-white/40">
      {new Date(props.date * 1000).toLocaleDateString()}
    </span>
  );
}
