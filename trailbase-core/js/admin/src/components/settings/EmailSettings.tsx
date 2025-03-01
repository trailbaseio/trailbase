import { createForm } from "@tanstack/solid-form";

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Label } from "@/components/ui/label";

import {
  unsetOrLargerThanZero,
  unsetOrNotEmptyValidator,
  buildTextAreaFormField,
  buildOptionalNumberFormField,
  buildOptionalTextFormField,
} from "@/components/FormFields";
import type { FormType } from "@/components/FormFields";
import { Config, EmailConfig } from "@proto/config";
import { createConfigQuery, setConfig } from "@/lib/config";

function EmailTemplate(props: {
  form: FormType<EmailConfig>;
  fieldName: string;
}) {
  return (
    <div class="my-2 mr-1 flex flex-col gap-4">
      <props.form.Field
        name={`${props.fieldName}.subject`}
        validators={unsetOrNotEmptyValidator()}
      >
        {buildOptionalTextFormField({
          label: textLabel("Subject"),
          info: (
            <p>
              Email's subject line. Valid template parameters:{" "}
              <span class="rounded bg-gray-200 font-mono">
                {"{{APP_NAME}}"}
              </span>
              .
            </p>
          ),
        })}
      </props.form.Field>

      <props.form.Field
        name="userVerificationTemplate.body"
        validators={unsetOrNotEmptyValidator()}
      >
        {buildTextAreaFormField(
          {
            label: textLabel("Body"),
            info: (
              <p>
                Email's body. Valid template parameters:{" "}
                <span class="rounded bg-gray-200 font-mono">
                  {"{{ APP_NAME }}"}
                </span>
                ,{" "}
                <span class="rounded bg-gray-200 font-mono">
                  {"{{ SITE_URL }}"}
                </span>
                , and{" "}
                <span class="rounded bg-gray-200 font-mono">
                  {"{{ CODE }}"}
                </span>
                .
              </p>
            ),
          },
          10,
        )}
      </props.form.Field>
    </div>
  );
}

export function EmailSettings(props: {
  markDirty: () => void;
  postSubmit: () => void;
}) {
  const config = createConfigQuery();

  const Form = (p: { config: EmailConfig }) => {
    const form = createForm<EmailConfig>(() => ({
      defaultValues: p.config,
      onSubmit: async ({ value }) => {
        const c = config.data?.config;
        if (!c) {
          console.warn("Missing base config.");
          return;
        }

        const newConfig = Config.fromPartial(c);
        newConfig.email = value;
        await setConfig(newConfig);

        props.postSubmit();
      },
    }));

    form.useStore((state) => {
      if (state.isDirty && !state.isSubmitted) {
        props.markDirty();
      }
    });

    return (
      <form
        onSubmit={(e) => {
          e.preventDefault();
          e.stopPropagation();
          form.handleSubmit();
        }}
      >
        <div class="flex flex-col gap-4">
          <Card id="smtp">
            <CardHeader>
              <h2>SMTP Settings</h2>
            </CardHeader>

            <CardContent class={flexColStyle}>
              <p class="mb-4 text-sm">
                TrailBase try to use the local sendmail command if no SMTP
                server is configured. This may be fine for development but will
                likely result in your Emails getting classified as Spam. Please
                add a valid SMTP server before going to production. There are
                many specialized providers with generous free tiers such as{" "}
                <a href="https://www.brevo.com/">Brevo</a>, ...
              </p>

              <form.Field
                name="smtpHost"
                validators={unsetOrNotEmptyValidator()}
              >
                {buildOptionalTextFormField({ label: textLabel("Host") })}
              </form.Field>

              <form.Field name="smtpPort" validators={unsetOrLargerThanZero()}>
                {buildOptionalNumberFormField({
                  integer: true,
                  label: textLabel("Port"),
                })}
              </form.Field>

              <form.Field
                name="smtpUsername"
                validators={unsetOrNotEmptyValidator()}
              >
                {buildOptionalTextFormField({
                  label: textLabel("Username"),
                  autocomplete: "username",
                })}
              </form.Field>

              <form.Field
                name="smtpPassword"
                validators={unsetOrNotEmptyValidator()}
              >
                {
                  // NOTE: we're not using buildSecretFormField here because it doesn't support optional.
                  buildOptionalTextFormField({
                    type: "password",
                    autocomplete: "current-password",
                    label: textLabel("Password"),
                  })
                }
              </form.Field>
            </CardContent>
          </Card>

          <Card id="sender">
            <CardHeader>
              <h2>Sender Settings</h2>
            </CardHeader>

            <CardContent class={flexColStyle}>
              <form.Field
                name="senderAddress"
                validators={unsetOrNotEmptyValidator()}
              >
                {buildOptionalTextFormField({
                  label: textLabel("Sender Address"),
                  type: "email",
                })}
              </form.Field>

              <form.Field name="senderName">
                {buildOptionalTextFormField({
                  label: textLabel("Sender Name"),
                })}
              </form.Field>
            </CardContent>
          </Card>

          <Card id="templates">
            <CardHeader>
              <h2>Email Templates</h2>
            </CardHeader>

            <CardContent>
              <Accordion multiple={true} collapsible class="w-full">
                <AccordionItem value="item-email-verification">
                  <AccordionTrigger>
                    Email Verification Template
                  </AccordionTrigger>

                  <AccordionContent>
                    <EmailTemplate
                      form={form}
                      fieldName="userVerificationTemplate"
                    />
                  </AccordionContent>
                </AccordionItem>

                <AccordionItem value="item-password-reset">
                  <AccordionTrigger>Password Reset Template</AccordionTrigger>

                  <AccordionContent>
                    <EmailTemplate
                      form={form}
                      fieldName="passwordResetTemplate"
                    />
                  </AccordionContent>
                </AccordionItem>
              </Accordion>
            </CardContent>
          </Card>

          <div class="flex justify-end">
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

  const emailConfig = () => {
    const c = config.data?.config?.email;
    if (c) {
      // "deep-copy"
      return EmailConfig.decode(EmailConfig.encode(c).finish());
    }

    // Fallback
    return EmailConfig.fromJSON({});
  };

  return <Form config={emailConfig()} />;
}

function textLabel(label: string) {
  return () => (
    <div class="w-40">
      <Label>{label}</Label>
    </div>
  );
}

const flexColStyle = "flex flex-col gap-2";
