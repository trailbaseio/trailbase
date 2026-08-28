import { render } from "@solidjs/testing-library"
import { expect, test } from "vitest"

import {
  TextFieldDescription,
  TextFieldErrorMessage,
} from "@/components/ui/text-field"

test("uses muted foreground for descriptions and destructive foreground for errors", () => {
  const result = render(() => (
    <>
      <TextFieldDescription>Description</TextFieldDescription>
      <TextFieldErrorMessage>Error</TextFieldErrorMessage>
    </>
  ))

  expect(result.getByText("Description")).toHaveClass("text-muted-foreground")
  expect(result.getByText("Error")).toHaveClass("text-destructive")
})
