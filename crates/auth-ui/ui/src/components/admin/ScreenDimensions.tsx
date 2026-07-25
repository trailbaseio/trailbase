import { isServer } from "solid-js/web";

export function ScreenDimensions() {
  if (isServer) return null;

  return (
    <p>
      {window.innerWidth}x{window.innerHeight} (WxH)
    </p>
  );
}
