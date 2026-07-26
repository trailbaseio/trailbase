import { render } from "solid-js/web";

import { ScreenDimensions } from "@/components/admin/ScreenDimensions";
import { Settings } from "@/components/admin/Settings";

export function renderAdminUI(id: string) {
  render(
    () => (
      <>
        <Settings />
        <ScreenDimensions />
      </>
    ),
    document.getElementById(id)!,
  );
}
