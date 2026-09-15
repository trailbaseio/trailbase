import { createForm } from "@tanstack/solid-form";
import { useQueryClient } from "@tanstack/solid-query";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Label } from "@/components/ui/label";

import {
  unsetOrNotEmptyValidator,
  buildOptionalTextFormField,
} from "@/components/FormFields";

import { Config, ServerConfig } from "@proto/config";
import { createConfigQuery, setConfig } from "@/lib/api/config";

export function FileSettings(props: {
  markDirty: () => void;
  postSubmit: () => void;
}) {
  const queryClient = useQueryClient();
  const config = createConfigQuery();

  const Form = (p: { config: ServerConfig }) => {
    const form = createForm(() => ({
      defaultValues: p.config satisfies ServerConfig,
      onSubmit: async ({ value }) => {
        const c = config.data?.config;
        if (!c) {
          console.warn("Missing base config.");
          return;
        }

        const newConfig = Config.fromPartial(c);
        newConfig.server = value;
        await setConfig({
          client: queryClient,
          config: newConfig,
          throw: true,
        });

        props.postSubmit();
      },
    }));

    form.useSelector((state) => {
      if (state.isDirty && !state.isSubmitted) {
        props.markDirty();
      }
    });

    return (
      <form
        method="dialog"
        onSubmit={(e: SubmitEvent) => {
          e.preventDefault();
          form.handleSubmit();
        }}
      >
        <div class="flex flex-col gap-4">
          <Card id="sender">
            <CardHeader>
              <h2>Object Storage (S3)</h2>
            </CardHeader>

            <CardContent class={flexColStyle}>
              <p class="mb-4 text-sm">
                If no explicit object storage is configured, the contents of
                uploaded files will be stored in{" "}
                <span class="font-mono">{"<depot>/uploads/"}</span>. When
                changing the object storage configuration, a server restart is
                currently required for the changes to take effect.
              </p>

              <form.Field
                name="s3StorageConfig.endpoint"
                validators={{
                  onChange: ({ value }: { value: string | undefined }) => {
                    if (value === undefined) return undefined;

                    try {
                      new URL(value);
                    } catch {
                      return `invalid url`;
                    }
                  },
                }}
              >
                {buildOptionalTextFormField({
                  label: textLabel("Endpoint URL"),
                  type: "text",
                })}
              </form.Field>

              <form.Field
                name="s3StorageConfig.region"
                validators={{
                  onChange: ({ value }: { value: string | undefined }) => {
                    if (value === undefined) return undefined;

                    if (!value.match(/^[a-z]{2}(-gov)?-[a-z]+-\d{1,2}$/)) {
                      return "expected value like 'eu-west-2'";
                    }
                  },
                }}
              >
                {buildOptionalTextFormField({
                  label: textLabel("Region"),
                  type: "text",
                })}
              </form.Field>

              <form.Field
                name="s3StorageConfig.bucketName"
                validators={{
                  onChange: ({ value }: { value: string | undefined }) => {
                    if (value === undefined) return undefined;

                    const pattern =
                      /^(?!\d{1,3}(\.\d{1,3}){3}$)(?!xn--)(?!.*\.\.)(?!.*\.-)(?!.*-\.)[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]$/;
                    if (!value.match(pattern)) {
                      return "invalid bucket name";
                    }
                  },
                }}
              >
                {buildOptionalTextFormField({
                  label: textLabel("Bucket name"),
                  type: "text",
                })}
              </form.Field>

              <form.Field
                name="s3StorageConfig.accessKey"
                validators={unsetOrNotEmptyValidator()}
              >
                {buildOptionalTextFormField({
                  label: textLabel("Access key"),
                  type: "text",
                })}
              </form.Field>

              <form.Field
                name="s3StorageConfig.secretAccessKey"
                validators={unsetOrNotEmptyValidator()}
              >
                {buildOptionalTextFormField({
                  label: textLabel("Secret access key"),
                  type: "text",
                })}
              </form.Field>
            </CardContent>
          </Card>

          <div class="flex justify-end gap-4">
            <form.Subscribe
              selector={(state) => ({
                canSubmit: state.canSubmit,
                isSubmitting: state.isSubmitting,
              })}
            >
              {(state) => {
                return (
                  <Button
                    type="submit"
                    disabled={!state().canSubmit}
                    variant="default"
                  >
                    {state().isSubmitting ? "..." : "Submit"}
                  </Button>
                );
              }}
            </form.Subscribe>
          </div>
        </div>
      </form>
    );
  };

  const serverConfig = () => {
    const c = config.data?.config?.server;
    if (c) {
      // "deep-copy"
      return ServerConfig.decode(ServerConfig.encode(c).finish());
    }

    // Fallback
    return ServerConfig.fromJSON({});
  };

  return <Form config={serverConfig()} />;
}

function textLabel(label: string) {
  return () => (
    <div class="w-40">
      <Label>{label}</Label>
    </div>
  );
}

const flexColStyle = "flex flex-col gap-2";
