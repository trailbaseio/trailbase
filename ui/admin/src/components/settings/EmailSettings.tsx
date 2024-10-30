import type { JSXElement } from "solid-js";
import { createForm } from "@tanstack/solid-form";

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";

import {
  notEmptyValidator,
  buildTextFormField,
  buildTextAreaFormField,
  buildNumberFormField,
  largerThanZero,
  buildSecretFormField,
} from "@/components/FormFields";
import type { FormType } from "@/components/FormFields";
import { Config, EmailConfig } from "@proto/config";
import { createConfigQuery, setConfig } from "@/lib/config";

function EmailTemplate(props: {
  form: FormType<EmailConfig>;
  fieldName: string;
}) {
  const form = props.form;

  return (
    <div class="my-2 mr-1 flex flex-col gap-4">
      <form.Field
        name={`${props.fieldName}.subject`}
        validators={notEmptyValidator()}
      >
        {buildTextFormField({
          label: () => <L>Subject</L>,
          info: (
            <p>
              Email's subject line. Valid template parameters:{" "}
              <span class="font-mono bg-gray-200 rounded">
                {"{{APP_NAME}}"}
              </span>
              .
            </p>
          ),
        })}
      </form.Field>

      <form.Field
        name="userVerificationTemplate.body"
        validators={notEmptyValidator()}
      >
        {buildTextAreaFormField(
          {
            label: () => <L>Body</L>,
            info: (
              <p>
                Email's body. Valid template parameters:{" "}
                <span class="font-mono bg-gray-200 rounded">
                  {"{{ APP_NAME }}"}
                </span>
                ,{" "}
                <span class="font-mono bg-gray-200 rounded">
                  {"{{ SITE_URL }}"}
                </span>
                , and{" "}
                <span class="font-mono bg-gray-200 rounded">
                  {"{{ CODE }}"}
                </span>
                .
              </p>
            ),
          },
          10,
        )}
      </form.Field>
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
        <div id="templates" class="flex flex-col gap-4">
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

              <form.Field name="smtpHost" validators={notEmptyValidator()}>
                {buildTextFormField({ label: () => <L>Host</L> })}
              </form.Field>

              <form.Field name="smtpPort" validators={largerThanZero()}>
                {buildNumberFormField({
                  integer: true,
                  label: () => <L>Port</L>,
                })}
              </form.Field>

              <form.Field name="smtpUsername" validators={notEmptyValidator()}>
                {buildTextFormField({ label: () => <L>Username</L> })}
              </form.Field>

              <form.Field name="smtpPassword" validators={notEmptyValidator()}>
                {buildSecretFormField({
                  label: () => <L>Password</L>,
                })}
              </form.Field>
            </CardContent>
          </Card>

          <Card id="sender">
            <CardHeader>
              <h2>Sender Settings</h2>
            </CardHeader>

            <CardContent class={flexColStyle}>
              <form.Field name="senderAddress" validators={notEmptyValidator()}>
                {buildTextFormField({
                  label: () => <L>Sender Address</L>,
                  type: "email",
                })}
              </form.Field>

              <form.Field name="senderName">
                {buildTextFormField({ label: () => <L>Sender Name</L> })}
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

          <div class="flex justify-end pt-4">
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
    return EmailConfig.create();
  };

  return <Form config={emailConfig()} />;
}

function L(props: { children: JSXElement }) {
  return <div class="w-40">{props.children}</div>;
}

const flexColStyle = "flex flex-col gap-2";
