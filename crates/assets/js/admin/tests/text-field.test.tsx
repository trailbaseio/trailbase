import { render } from "@solidjs/testing-library"
import { expect, test } from "vitest"

import {
  TextField,
  TextFieldDescription,
  TextFieldErrorMessage,
} from "@/components/ui/text-field"

test("uses muted foreground for descriptions and destructive foreground for errors", () => {
  const result = render(() => (
    <TextField validationState="invalid">
      <TextFieldDescription>Description</TextFieldDescription>
      <TextFieldErrorMessage>Error</TextFieldErrorMessage>
    </TextField>
  ))

  expect(result.getByText("Description")).toHaveClass("text-muted-foreground")
  expect(result.getByText("Error")).toHaveClass("text-destructive")
})
