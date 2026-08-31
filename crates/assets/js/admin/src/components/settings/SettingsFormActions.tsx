import { Button } from "@/components/ui/button";

export function SettingsFormActions(props: {
  dirty: boolean;
  canSubmit: boolean;
  isSubmitting: boolean;
  onReset: () => void;
}) {
  return (
    <div class="sticky bottom-0 flex w-full flex-col-reverse gap-2 bg-background py-4 sm:flex-row sm:justify-end">
      {props.dirty && (
        <>
          <Button
            type="button"
            variant="outline"
            class="w-full sm:w-auto"
            onClick={props.onReset}
            disabled={props.isSubmitting}
          >
            Reset
          </Button>
          <Button
            type="submit"
            class="w-full sm:w-auto"
            disabled={!props.canSubmit || props.isSubmitting}
          >
            Save changes
          </Button>
        </>
      )}
    </div>
  );
}
