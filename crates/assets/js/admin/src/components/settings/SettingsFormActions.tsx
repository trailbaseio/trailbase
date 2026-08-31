import { Button } from "@/components/ui/button";

export function SettingsFormActions(props: {
  dirty: boolean;
  canSubmit: boolean;
  isSubmitting: boolean;
  onReset: () => void;
}) {
  return (
    <div class="sticky bottom-0 flex justify-end gap-4 bg-background py-4">
      {props.dirty && (
        <>
          <Button type="button" variant="outline" onClick={props.onReset} disabled={props.isSubmitting}>
            Reset
          </Button>
          <Button type="submit" disabled={!props.canSubmit || props.isSubmitting}>
            {props.isSubmitting ? "..." : "Save changes"}
          </Button>
        </>
      )}
    </div>
  );
}
