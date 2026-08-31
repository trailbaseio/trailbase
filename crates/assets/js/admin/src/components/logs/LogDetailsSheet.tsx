import { For } from "solid-js";
import type { JSX } from "solid-js";

import type { LogJson } from "@bindings/LogJson";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { copyToClipboard } from "@/lib/utils";
import { formatLogLatency, formatLogTimestamp } from "@/components/logs/logs";

const emptyValue = "—";

type Detail = {
  name: string;
  value: string;
  display?: string;
};

function DetailList(props: {
  heading: string;
  details: Detail[];
}): JSX.Element {
  return (
    <section class="space-y-2">
      <h3 class="text-sm font-semibold">{props.heading}</h3>
      <dl class="divide-y rounded-md border text-sm">
        <For each={props.details}>
          {(detail) => (
            <div class="grid grid-cols-[minmax(0,1fr)_auto] gap-3 px-3 py-2">
              <dt class="text-muted-foreground">{detail.name}</dt>
              <dd class="flex min-w-0 items-center gap-2 text-right">
                <span class="max-w-[330px] wrap-break-word">
                  {detail.display ?? detail.value}
                </span>
                {detail.value !== "" && (
                  <button
                    type="button"
                    class="text-muted-foreground text-xs underline underline-offset-2"
                    aria-label={`Copy ${detail.name}`}
                    onClick={() => {
                      void copyToClipboard(
                        detail.value,
                        false,
                        `${detail.name} copied`,
                      );
                    }}
                  >
                    Copy
                  </button>
                )}
              </dd>
            </div>
          )}
        </For>
      </dl>
    </section>
  );
}

const text = (value: string | null | undefined): string => value || emptyValue;
const detail = (
  name: string,
  value: string | null | undefined,
  display?: string,
): Detail => ({
  name,
  value: value ?? "",
  display: display ?? text(value),
});

export function LogDetailsSheet(props: {
  log: LogJson | undefined;
  onClose: () => void;
}) {
  const log = () => props.log;
  const details = (): DetailListProps[] => {
    const current = log();
    if (!current) return [];
    const timestamp = formatLogTimestamp(current.created);
    const city = current.client_geoip_city?.name;
    const country =
      current.client_geoip_city?.country_code ?? current.client_geoip_cc;
    return [
      {
        heading: "Request",
        details: [
          detail("id", current.id.toString()),
          detail("method", current.method),
          detail("url", current.url),
          detail("status", current.status.toString()),
        ],
      },
      {
        heading: "Timing",
        details: [
          detail("created", current.created.toString(), timestamp.iso),
          detail(
            "latency_ms",
            current.latency_ms.toString(),
            formatLogLatency(current.latency_ms),
          ),
        ],
      },
      {
        heading: "Client and location",
        details: [
          detail("client_ip", current.client_ip),
          detail("client_geoip_cc", current.client_geoip_cc),
          detail(
            "client_geoip_city",
            city && country ? `${city}, ${country}` : (city ?? country),
          ),
        ],
      },
      {
        heading: "Identity",
        details: [detail("user_id", current.user_id)],
      },
      {
        heading: "Metadata",
        details: [
          detail("referer", current.referer),
          detail("user_agent", current.user_agent),
        ],
      },
    ];
  };

  return (
    <Sheet
      open={props.log !== undefined}
      onOpenChange={(open) => !open && props.onClose()}
    >
      <SheetContent class="w-full sm:max-w-[520px]">
        <SheetHeader>
          <SheetTitle class="pr-8 break-all">
            {log() ? `${log()!.method} ${log()!.url}` : "Request details"}
          </SheetTitle>
          <SheetDescription>Request details</SheetDescription>
        </SheetHeader>
        <div class="mt-6 space-y-6">
          <For each={details()}>
            {(group) => (
              <DetailList heading={group.heading} details={group.details} />
            )}
          </For>
        </div>
      </SheetContent>
    </Sheet>
  );
}

type DetailListProps = { heading: string; details: Detail[] };
