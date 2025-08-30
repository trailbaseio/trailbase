import { createSignal } from "solid-js";
import { createForm } from "@tanstack/solid-form";
import { useQueryClient } from "@tanstack/solid-query";
import { useStore } from "@nanostores/solid";

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { showToast } from "@/components/ui/toast";
import {
  TextField,
  TextFieldLabel,
  TextFieldInput,
} from "@/components/ui/text-field";

import {
  unsetOrLargerThanZero,
  unsetOrNotEmptyValidator,
  buildOptionalTextAreaFormField,
  buildOptionalNumberFormField,
  buildOptionalTextFormField,
} from "@/components/FormFields";
import type { FormApiT } from "@/components/FormFields";

import type { TestEmailRequest } from "@bindings/TestEmailRequest";

import { Config, EmailConfig } from "@proto/config";
import { createConfigQuery, setConfig } from "@/lib/config";
import { $user, adminFetch } from "@/lib/fetch";

import DEFAULT_EMAIL_VERIFICATION_SUBJECT from "@templates/default_email_verification_subject.txt?raw";
import DEFAULT_EMAIL_VERIFICATION_BODY from "@templates/default_email_verification_body.html?raw";

import DEFAULT_EMAIL_CHANGE_ADDRESS_BODY from "@templates/default_email_change_address_body.html?inline?raw";
import DEFAULT_EMAIL_CHANGE_ADDRESS_SUBJECT from "@templates/default_email_change_address_subject.txt?inline?raw";

import DEFAULT_EMAIL_RESET_PASSWORD_SUBJECT from "@templates/default_email_reset_password_subject.txt?raw";
import DEFAULT_EMAIL_RESET_PASSWORD_BODY from "@templates/default_email_reset_password_body.html?raw";

function EmailTemplate(props: {
  form: FormApiT<EmailConfig>;
  fieldName: string;
  subjectPlaceholder?: string;
  bodyPlaceholder?: string;
}) {
  const Parameter = (props: { label: string }) => (
    <>
      {" "}
      <span class="text-nowrap rounded bg-gray-200 font-mono">
        {`{{ ${props.label} }}`}
      </span>{" "}
    </>
  );

  return (
    <div class="my-2 mr-1 flex flex-col gap-4">
      <props.form.Field
        name={`${props.fieldName}.subject`}
        validators={unsetOrNotEmptyValidator()}
      >
        {buildOptionalTextFormField({
          label: textLabel("Subject"),
          placeholder: props.subjectPlaceholder,
          info: (
            <p>
              Valid parameters: <Parameter label="APP_NAME" />
              and
              <Parameter label="EMAIL" />.
            </p>
          ),
        })}
      </props.form.Field>

      <props.form.Field
        name={`${props.fieldName}.body`}
        validators={unsetOrNotEmptyValidator()}
      >
        {buildOptionalTextAreaFormField(
          {
            label: textLabel("Body"),
            placeholder: props.bodyPlaceholder,
            info: (
              <p>
                Valid parameters: <Parameter label="APP_NAME" />
                ,
                <Parameter label="SITE_URL" />
                ,
                <Parameter label="EMAIL" />
                ,
                <Parameter label="VERIFICATION_URL" />
                and
                <Parameter label="CODE" />.
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
  const queryClient = useQueryClient();
  const config = createConfigQuery();

  const [dialogOpen, setDialogOpen] = createSignal(false);

  const Form = (p: { config: EmailConfig }) => {
    const form = createForm(() => ({
      defaultValues: p.config satisfies EmailConfig,
      onSubmit: async ({ value }) => {
        const c = config.data?.config;
        if (!c) {
          console.warn("Missing base config.");
          return;
        }

        const newConfig = Config.fromPartial(c);
        newConfig.email = value;
        await setConfig(queryClient, newConfig);

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
        method="dialog"
        onSubmit={(e: SubmitEvent) => {
          e.preventDefault();
          form.handleSubmit();
        }}
      >
        <div class="flex flex-col gap-4">
          <Card id="smtp">
            <CardHeader>
              <h2>SMTP</h2>
            </CardHeader>

            <CardContent class={flexColStyle}>
              <p class="mb-4 text-sm">
                The SMTP server to be used for email delivery. When no SMTP is
                configured, your local <span class="font-mono">sendmail</span>{" "}
                will be used. Before going to production, please make sure to
                set up a suitable SMTP server. Otherwise, your emails will
                likely get classified as Spam.{" "}
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
              <h2>Sender</h2>
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
              <h2>Templates</h2>
            </CardHeader>

            <CardContent>
              <Accordion multiple={true} collapsible class="w-full">
                <AccordionItem value="item-email-verification">
                  <AccordionTrigger>Email Verification</AccordionTrigger>

                  <AccordionContent>
                    <EmailTemplate
                      form={form}
                      fieldName="userVerificationTemplate"
                      subjectPlaceholder={DEFAULT_EMAIL_VERIFICATION_SUBJECT}
                      bodyPlaceholder={DEFAULT_EMAIL_VERIFICATION_BODY}
                    />
                  </AccordionContent>
                </AccordionItem>

                <AccordionItem value="item-change-email">
                  <AccordionTrigger>Change Email Address</AccordionTrigger>

                  <AccordionContent>
                    <EmailTemplate
                      form={form}
                      fieldName="changeEmailTemplate"
                      subjectPlaceholder={DEFAULT_EMAIL_CHANGE_ADDRESS_SUBJECT}
                      bodyPlaceholder={DEFAULT_EMAIL_CHANGE_ADDRESS_BODY}
                    />
                  </AccordionContent>
                </AccordionItem>

                <AccordionItem value="item-password-reset">
                  <AccordionTrigger>Password Reset</AccordionTrigger>

                  <AccordionContent>
                    <EmailTemplate
                      form={form}
                      fieldName="passwordResetTemplate"
                      subjectPlaceholder={DEFAULT_EMAIL_RESET_PASSWORD_SUBJECT}
                      bodyPlaceholder={DEFAULT_EMAIL_RESET_PASSWORD_BODY}
                    />
                  </AccordionContent>
                </AccordionItem>
              </Accordion>
            </CardContent>
          </Card>

          <div class="flex justify-end gap-4">
            <Dialog
              id="confirm"
              modal={true}
              open={dialogOpen()}
              onOpenChange={setDialogOpen}
            >
              <TestEmailDialog closeDialog={() => setDialogOpen(false)} />

              <Button
                type="button"
                variant="outline"
                onClick={() => setDialogOpen(true)}
              >
                Send Test Email
              </Button>
            </Dialog>

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

function TestEmailDialog(props: { closeDialog: () => void }) {
  const user = useStore($user);
  let email: HTMLInputElement | undefined;

  return (
    <DialogContent>
      <form
        method="dialog"
        onSubmit={(e: SubmitEvent) => {
          e.preventDefault();

          const emailAddress = email?.value;
          if (!emailAddress) return;

          adminFetch("/email/test", {
            method: "POST",
            body: JSON.stringify({
              email_address: emailAddress,
            } as TestEmailRequest),
            throwOnError: true,
          });

          props.closeDialog();

          showToast({
            title: `Sent to ${emailAddress}`,
            variant: "success",
          });
        }}
      >
        <DialogTitle>Send Test Email</DialogTitle>

        <div class="my-4 flex flex-col gap-4">
          <p class="text-sm">
            A default test subject and body will be used to avoid abuse.
          </p>

          <TextField class="flex items-center gap-2">
            <TextFieldLabel class="w-[108px]">Email</TextFieldLabel>

            <TextFieldInput
              type="email"
              value={user()?.email ?? ""}
              placeholder="Email"
              autocomplete="username"
              ref={email}
            />
          </TextField>
        </div>

        <DialogFooter>
          <div class="flex w-full justify-between gap-4">
            <Button type="button" onClick={props.closeDialog} variant="outline">
              Close
            </Button>

            <Button type="submit">Send</Button>
          </div>
        </DialogFooter>
      </form>
    </DialogContent>
  );
}

function textLabel(label: string) {
  return () => (
    <div class="w-40">
      <Label>{label}</Label>
    </div>
  );
}

const flexColStyle = "flex flex-col gap-2";
