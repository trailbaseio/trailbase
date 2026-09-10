import { urlSafeBase64Decode } from "trailbase";
import { Match, Switch } from "solid-js";

import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { BlobEncoding } from "@/components/table/BlobEncoding";

import { urlSafeBase64ToUuid, toHex } from "@/lib/utils";

export function Uuid(props: {
  base64UrlSafeBlob: string;
  blobEncoding: BlobEncoding;
}) {
  const render = () => {
    if (props.blobEncoding === "hex") {
      return toHex(urlSafeBase64Decode(props.base64UrlSafeBlob));
    }
    return props.base64UrlSafeBlob;
  };

  return (
    <Tooltip>
      <TooltipTrigger as="div">
        <div class="font-mono text-xs text-wrap">
          <Switch>
            <Match when={props.blobEncoding === "mixed"}>
              <div class="md:w-[260px]">
                {urlSafeBase64ToUuid(props.base64UrlSafeBlob)}
              </div>
            </Match>

            <Match when={true}>{render()}</Match>
          </Switch>
        </div>
      </TooltipTrigger>

      <TooltipContent>
        <div>
          <ul>
            <li>
              UUID:{" "}
              <span class="font-bold">
                {urlSafeBase64ToUuid(props.base64UrlSafeBlob)}
              </span>
            </li>
            <li>
              Url-safe base64:{" "}
              <span class="font-bold">{props.base64UrlSafeBlob}</span>
            </li>
          </ul>
        </div>
      </TooltipContent>
    </Tooltip>
  );
}
