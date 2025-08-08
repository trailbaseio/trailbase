import { createSignal, For } from "solid-js";
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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

import {
  unsetOrLargerThanZero,
  unsetOrNotEmptyValidator,
  buildTextAreaFormField,
  buildOptionalNumberFormField,
  buildOptionalTextFormField,
  buildSimpleOptionalTextField,
  buildSimpleOptionalNumberField,
  buildSimpleOptionalTextArea,
} from "@/components/FormFields";
import type { FormApiT } from "@/components/FormFields";

import type { TestEmailRequest } from "@bindings/TestEmailRequest";

import { Config, EmailConfig, SmtpEncryption } from "@proto/config";
import { createConfigQuery, setConfig } from "@/lib/config";
import { $user, adminFetch } from "@/lib/fetch";

function EmailTemplate(props: {
  form: FormApiT<EmailConfig>;
  fieldName: string;
}) {
  return (
    <div class="my-2 mr-1 flex flex-col gap-4">
      <props.form.Field
        name={`${props.fieldName}.subject`}
        validators={unsetOrNotEmptyValidator()}
      >
        {buildSimpleOptionalTextField({
          label: textLabel("Subject"),
          placeholder: "Email subject line",
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
        name={`${props.fieldName}.body`}
        validators={unsetOrNotEmptyValidator()}
      >
        {buildSimpleOptionalTextArea({
          label: textLabel("Body"),
          rows: 10,
          placeholder: "Email body HTML",
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
        })}
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

        console.log("Submitting email config:", value);
        console.log("Encryption value:", value.smtpEncryption);
        
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

              <form.Field name="smtpHost">
                {buildSimpleOptionalTextField({ 
                  label: textLabel("Host"),
                  placeholder: "smtp.example.com"
                })}
              </form.Field>

              <form.Field name="smtpPort">
                {buildSimpleOptionalNumberField({
                  integer: true,
                  label: textLabel("Port"),
                  placeholder: "587"
                })}
              </form.Field>

              <form.Field name="smtpEncryption">
                {(field) => {
                  // Default to NONE (1) if not set
                  const fieldValue = () => field().state.value ?? SmtpEncryption.SMTP_ENCRYPTION_NONE;
                  
                  return (
                    <TextField class="w-full">
                      <div
                        class="grid items-center gap-x-2 gap-y-1"
                        style={{ "grid-template-columns": "auto 1fr" }}
                      >
                        <div class="w-40">
                          <TextFieldLabel>Encryption</TextFieldLabel>
                        </div>
                        <div class="w-full">
                          <Select
                            value={fieldValue().toString()}
                            onChange={(value) => {
                              const strValue = value?.toString() ?? "1";
                              const numValue = parseInt(strValue);
                              field().handleChange(numValue);
                            }}
                            options={[
                              SmtpEncryption.SMTP_ENCRYPTION_NONE.toString(),
                              SmtpEncryption.SMTP_ENCRYPTION_STARTTLS.toString(),
                              SmtpEncryption.SMTP_ENCRYPTION_TLS.toString(),
                            ]}
                            placeholder="Select encryption type"
                            itemComponent={(props) => (
                              <SelectItem item={props.item}>
                                {props.item.rawValue === "1" && "None (Plain)"}
                                {props.item.rawValue === "2" && "STARTTLS"}
                                {props.item.rawValue === "3" && "TLS/SSL"}
                              </SelectItem>
                            )}
                          >
                            <SelectTrigger class="flex h-10 w-full items-center justify-between rounded-md border border-input bg-transparent px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50">
                              <SelectValue<string>>
                                {(state) => {
                                  const currentVal = fieldValue().toString();
                                  if (currentVal === "1") return "None (Plain)";
                                  if (currentVal === "2") return "STARTTLS";
                                  if (currentVal === "3") return "TLS/SSL";
                                  return "None (Plain)";
                                }}
                              </SelectValue>
                            </SelectTrigger>
                            <SelectContent />
                          </Select>
                        </div>
                      </div>
                    </TextField>
                  );
                }}
              </form.Field>

              <form.Field name="smtpUsername">
                {buildSimpleOptionalTextField({
                  label: textLabel("Username (optional)"),
                  autocomplete: "username",
                  placeholder: "Leave empty if no auth required"
                })}
              </form.Field>

              <form.Field name="smtpPassword">
                {buildSimpleOptionalTextField({
                  type: "password",
                  autocomplete: "current-password",
                  label: textLabel("Password (optional)"),
                  placeholder: "Leave empty if no auth required"
                })}
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
                {buildSimpleOptionalTextField({
                  label: textLabel("Sender Address"),
                  type: "email",
                  placeholder: "noreply@example.com"
                })}
              </form.Field>

              <form.Field name="senderName">
                {buildSimpleOptionalTextField({
                  label: textLabel("Sender Name"),
                  placeholder: "Your Service Name"
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
