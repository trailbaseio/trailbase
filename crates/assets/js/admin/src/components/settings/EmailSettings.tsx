import {
  createSignal,
  For,
  Switch,
  Match,
  Show,
  createEffect,
  onCleanup,
  untrack,
} from "solid-js";
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
import { Callout, CalloutContent, CalloutTitle } from "@/components/ui/callout";
import { SettingsFormActions } from "@/components/settings/SettingsFormActions";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  TextField,
  TextFieldLabel,
  TextFieldInput,
} from "@/components/ui/text-field";

import {
  unsetOrLargerThanZero,
  unsetOrNotEmptyValidator,
  buildOptionalTextAreaFormField,
  buildOptionalSmallIntegerFormField,
  buildOptionalTextFormField,
} from "@/components/FormFields";
import type { FormApiT } from "@/components/FormFields";

import type { TestEmailRequest } from "@bindings/TestEmailRequest";

import { Config, EmailConfig, SmtpEncryption } from "@proto/config";
import { createConfigQuery, setConfig } from "@/lib/api/config";
import { $user } from "@/lib/client";
import { adminFetch } from "@/lib/fetch";

import DEFAULT_EMAIL_VERIFICATION_SUBJECT from "@templates/default_email_verification_subject.txt?raw";
import DEFAULT_EMAIL_VERIFICATION_BODY from "@templates/default_email_verification_body.html?raw";

import DEFAULT_EMAIL_CHANGE_ADDRESS_BODY from "@templates/default_email_change_address_body.html?inline?raw";
import DEFAULT_EMAIL_CHANGE_ADDRESS_SUBJECT from "@templates/default_email_change_address_subject.txt?inline?raw";

import DEFAULT_EMAIL_RESET_PASSWORD_SUBJECT from "@templates/default_email_reset_password_subject.txt?raw";
import DEFAULT_EMAIL_RESET_PASSWORD_BODY from "@templates/default_email_reset_password_body.html?raw";

import DEFAULT_EMAIL_OTP_SUBJECT from "@templates/default_email_otp_subject.txt?raw";
import DEFAULT_EMAIL_OTP_BODY from "@templates/default_email_otp_body.html?raw";

function EmailTemplate(props: {
  form: FormApiT<EmailConfig>;
  fieldName: string;
  subjectPlaceholder?: string;
  bodyPlaceholder?: string;
  availableTemplateParams: string[];
}) {
  const Parameter = (props: { label: string }) => (
    <>
      {" "}
      <span class="bg-muted rounded-sm font-mono text-nowrap">
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
        {buildOptionalTextAreaFormField({
          label: textLabel("Body"),
          placeholder: props.bodyPlaceholder,
          info: (
            <p>
              Valid parameters:
              <For each={props.availableTemplateParams}>
                {(label, index) => (
                  <Switch>
                    <Match when={index() === 0}>
                      <Parameter label={label} />
                    </Match>

                    <Match
                      when={index() >= props.availableTemplateParams.length - 1}
                    >
                      and <Parameter label={label} />
                    </Match>

                    <Match when={true}>
                      , <Parameter label={label} />
                    </Match>
                  </Switch>
                )}
              </For>
            </p>
          ),
          rows: 10,
        })}
      </props.form.Field>
    </div>
  );
}

const emailScalarFields = [
  "smtpHost",
  "smtpPort",
  "smtpUsername",
  "smtpPassword",
  "smtpEncryption",
  "senderName",
  "senderAddress",
] as const;
const emailTemplateFields = [
  "userVerificationTemplate",
  "passwordResetTemplate",
  "changeEmailTemplate",
  "otpTemplate",
] as const;

type EmailTemplateValue = EmailConfig["userVerificationTemplate"];

function sameTemplateLeaves(
  left: EmailTemplateValue,
  right: EmailTemplateValue,
) {
  return left?.subject === right?.subject && left?.body === right?.body;
}

function sameEmailLeaves(left: EmailConfig, right: EmailConfig) {
  return (
    emailScalarFields.every((field) => left[field] === right[field]) &&
    emailTemplateFields.every((field) =>
      sameTemplateLeaves(left[field], right[field]),
    )
  );
}

function mergeTemplateLeaves(
  submitted: EmailTemplateValue,
  baseline: EmailTemplateValue,
  remote: EmailTemplateValue,
): EmailTemplateValue {
  const subjectChanged = submitted?.subject !== baseline?.subject;
  const bodyChanged = submitted?.body !== baseline?.body;
  if (!subjectChanged && !bodyChanged) return remote;
  return {
    subject: subjectChanged ? submitted?.subject : remote?.subject,
    body: bodyChanged ? submitted?.body : remote?.body,
  };
}

function mergeEmailLeaves(
  submitted: EmailConfig,
  baseline: EmailConfig,
  remote: EmailConfig,
) {
  return EmailConfig.fromPartial({
    smtpHost:
      submitted.smtpHost !== baseline.smtpHost
        ? submitted.smtpHost
        : remote.smtpHost,
    smtpPort:
      submitted.smtpPort !== baseline.smtpPort
        ? submitted.smtpPort
        : remote.smtpPort,
    smtpUsername:
      submitted.smtpUsername !== baseline.smtpUsername
        ? submitted.smtpUsername
        : remote.smtpUsername,
    smtpPassword:
      submitted.smtpPassword !== baseline.smtpPassword
        ? submitted.smtpPassword
        : remote.smtpPassword,
    smtpEncryption:
      submitted.smtpEncryption !== baseline.smtpEncryption
        ? submitted.smtpEncryption
        : remote.smtpEncryption,
    senderName:
      submitted.senderName !== baseline.senderName
        ? submitted.senderName
        : remote.senderName,
    senderAddress:
      submitted.senderAddress !== baseline.senderAddress
        ? submitted.senderAddress
        : remote.senderAddress,
    userVerificationTemplate: mergeTemplateLeaves(
      submitted.userVerificationTemplate,
      baseline.userVerificationTemplate,
      remote.userVerificationTemplate,
    ),
    passwordResetTemplate: mergeTemplateLeaves(
      submitted.passwordResetTemplate,
      baseline.passwordResetTemplate,
      remote.passwordResetTemplate,
    ),
    changeEmailTemplate: mergeTemplateLeaves(
      submitted.changeEmailTemplate,
      baseline.changeEmailTemplate,
      remote.changeEmailTemplate,
    ),
    otpTemplate: mergeTemplateLeaves(
      submitted.otpTemplate,
      baseline.otpTemplate,
      remote.otpTemplate,
    ),
  });
}

export function EmailSettings(props: {
  setDirty: (dirty: boolean) => void;
  postSubmit: (dirty?: boolean) => void;
}) {
  const queryClient = useQueryClient();
  const config = createConfigQuery();
  const [dialogOpen, setDialogOpen] = createSignal(false);
  const [submitError, setSubmitError] = createSignal(false);
  const clone = (value: EmailConfig) =>
    EmailConfig.decode(EmailConfig.encode(value).finish());

  const Form = (p: { config: EmailConfig }) => {
    const initial = clone(p.config);
    let latestRemote = clone(initial);
    let remoteRevision = 0;
    let editBaseline = clone(initial);
    let lastIncoming = clone(initial);
    let active = true;
    onCleanup(() => {
      active = false;
    });
    const form = createForm(() => ({
      defaultValues: clone(initial),
      onSubmit: async ({ value }) => {
        setSubmitError(false);
        const submitted = clone(value);
        const baselineAtSubmit = clone(editBaseline);
        const latestAtSubmit = clone(latestRemote);
        const revisionAtSubmit = remoteRevision;
        const merged = mergeEmailLeaves(
          submitted,
          baselineAtSubmit,
          latestAtSubmit,
        );
        const base = config.data?.config;
        if (!base) return;
        const newConfig = Config.fromPartial(base);
        newConfig.email = merged;
        try {
          await setConfig({
            client: queryClient,
            config: newConfig,
            throw: true,
          });
          if (!active) return;
          const saved = clone(
            remoteRevision === revisionAtSubmit
              ? merged
              : mergeEmailLeaves(submitted, baselineAtSubmit, latestRemote),
          );
          latestRemote = clone(saved);
          const current = formValues();
          const editedAfterSubmit = !sameEmailLeaves(current, submitted);
          editBaseline = clone(saved);
          if (!editedAfterSubmit) form.reset(clone(saved));
          props.postSubmit(editedAfterSubmit);
        } catch {
          if (active) setSubmitError(true);
        }
      },
    }));
    const formValues = form.useSelector((state) => state.values);
    const modified = () => !sameEmailLeaves(formValues(), editBaseline);
    createEffect(() => props.setDirty(modified()));
    createEffect(() => {
      const incoming = config.data?.config?.email ?? EmailConfig.fromJSON({});
      const next = clone(incoming);
      if (sameEmailLeaves(lastIncoming, next)) return;
      lastIncoming = next;
      remoteRevision += 1;
      const dirty = untrack(modified);
      latestRemote = clone(next);
      if (!dirty) {
        editBaseline = clone(next);
        form.reset(clone(next));
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
                {buildOptionalSmallIntegerFormField({
                  label: textLabel("Port"),
                  min: 1,
                  max: 65535,
                })}
              </form.Field>

              <form.Field name="smtpEncryption">
                {(field) => {
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
                          <SmtpEncryptionSelect
                            value={field().state.value}
                            handleChange={field().handleChange}
                          />
                        </div>
                      </div>
                    </TextField>
                  );
                }}
              </form.Field>

              <form.Field
                name="smtpUsername"
                validators={unsetOrNotEmptyValidator()}
              >
                {buildOptionalTextFormField({
                  label: textLabel("Username"),
                  autocomplete: "off",
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
                    autocomplete: "off",
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
              <p class="mb-4 text-sm">
                Template placeholders use {"{{ PARAMETER }}"}. Available
                parameters are listed in each template editor.
              </p>

              <Accordion multiple={true} collapsible class="w-full">
                <AccordionItem value="item-email-verification">
                  <AccordionTrigger>Email Verification</AccordionTrigger>

                  <AccordionContent>
                    <EmailTemplate
                      form={form}
                      fieldName="userVerificationTemplate"
                      subjectPlaceholder={DEFAULT_EMAIL_VERIFICATION_SUBJECT}
                      bodyPlaceholder={DEFAULT_EMAIL_VERIFICATION_BODY}
                      availableTemplateParams={[
                        "APP_NAME",
                        "EMAIL",
                        "REDIRECT_URI",
                        "SITE_URL",
                        "TOKEN",
                        "VERIFICATION_URL",
                      ]}
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
                      availableTemplateParams={[
                        "APP_NAME",
                        "EMAIL",
                        "REDIRECT_URI",
                        "SITE_URL",
                        "TOKEN",
                        "VERIFICATION_URL",
                      ]}
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
                      availableTemplateParams={[
                        "APP_NAME",
                        "EMAIL",
                        "SITE_URL",
                        "TOKEN",
                      ]}
                    />
                  </AccordionContent>
                </AccordionItem>

                <AccordionItem value="item-otp">
                  <AccordionTrigger>OTP Request</AccordionTrigger>

                  <AccordionContent>
                    <EmailTemplate
                      form={form}
                      fieldName="otpTemplate"
                      subjectPlaceholder={DEFAULT_EMAIL_OTP_SUBJECT}
                      bodyPlaceholder={DEFAULT_EMAIL_OTP_BODY}
                      availableTemplateParams={[
                        "APP_NAME",
                        "CODE",
                        "EMAIL",
                        "REDIRECT_URI",
                        "SITE_URL",
                      ]}
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
              {(state) => (
                <>
                  <Show when={submitError()}>
                    <Callout variant="error" role="alert">
                      <CalloutTitle>Unable to save settings</CalloutTitle>
                      <CalloutContent>
                        Check your values and try again.
                      </CalloutContent>
                    </Callout>
                  </Show>
                  <SettingsFormActions
                    dirty={modified()}
                    canSubmit={state().canSubmit}
                    isSubmitting={state().isSubmitting}
                    onReset={() => {
                      setSubmitError(false);
                      editBaseline = clone(latestRemote);
                      form.reset(clone(latestRemote));
                    }}
                  />
                </>
              )}
            </form.Subscribe>
          </div>
        </div>
      </form>
    );
  };

  return (
    <Switch>
      <Match when={config.isError}>
        <Callout variant="error" role="alert">
          <CalloutTitle>Unable to load settings</CalloutTitle>
          <CalloutContent>Please try again later.</CalloutContent>
        </Callout>
      </Match>
      <Match when={config.isLoading}>
        <div role="status">Loading settings...</div>
      </Match>
      <Match when={config.data?.config}>
        <Form config={config.data!.config!.email ?? EmailConfig.fromJSON({})} />
      </Match>
    </Switch>
  );
}

function TestEmailDialog(props: { closeDialog: () => void }) {
  const user = useStore($user);
  const [pending, setPending] = createSignal(false);
  const [error, setError] = createSignal(false);
  let active = true;
  let email: HTMLInputElement | undefined;
  onCleanup(() => {
    active = false;
  });

  return (
    <DialogContent>
      <form
        method="dialog"
        onSubmit={async (e: SubmitEvent) => {
          e.preventDefault();
          const emailAddress = email?.value;
          if (!emailAddress || pending()) return;
          setPending(true);
          setError(false);
          try {
            await adminFetch("/email/test", {
              method: "POST",
              body: JSON.stringify({
                email_address: emailAddress,
              } as TestEmailRequest),
              throwOnError: true,
            });
            if (!active) return;
            props.closeDialog();
            showToast({ title: `Sent to ${emailAddress}`, variant: "success" });
          } catch {
            if (active) setError(true);
          } finally {
            if (active) setPending(false);
          }
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

        <Show when={error()}>
          <Callout variant="error" role="alert">
            <CalloutTitle>Unable to send test email</CalloutTitle>
            <CalloutContent>
              Please check the address and try again.
            </CalloutContent>
          </Callout>
        </Show>
        <DialogFooter>
          <div class="flex w-full justify-between gap-4">
            <Button
              type="button"
              onClick={props.closeDialog}
              variant="outline"
              disabled={pending()}
            >
              Close
            </Button>
            <Button type="submit" disabled={pending()}>
              {pending() ? "Sending…" : "Send"}
            </Button>
            <Show when={pending()}>
              <div role="status" aria-live="polite">
                Sending…
              </div>
            </Show>
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

function SmtpEncryptionSelect(props: {
  value: SmtpEncryption | undefined;
  handleChange: (v: SmtpEncryption | undefined) => void;
  disabled?: boolean;
}) {
  return (
    <Select<SmtpEncryption | undefined>
      value={props.value}
      disabled={props.disabled}
      options={[
        undefined,
        // SmtpEncryption.SMTP_ENCRYPTION_STARTTLS,
        SmtpEncryption.SMTP_ENCRYPTION_TLS,
        SmtpEncryption.SMTP_ENCRYPTION_NONE,
      ]}
      placeholder={smtpEncryptionLabel(undefined)}
      itemComponent={(props) => (
        <SelectItem item={props.item}>
          {smtpEncryptionLabel(props.item.rawValue)}
        </SelectItem>
      )}
      onChange={(value) => {
        props.handleChange(value ?? SmtpEncryption.SMTP_ENCRYPTION_UNDEFINED);
      }}
    >
      <SelectTrigger>
        <SelectValue<SmtpEncryption>>
          {(_state) => smtpEncryptionLabel(props.value)}
        </SelectValue>
      </SelectTrigger>

      <SelectContent />
    </Select>
  );
}

function smtpEncryptionLabel(enc: SmtpEncryption | undefined): string {
  switch (enc) {
    case SmtpEncryption.SMTP_ENCRYPTION_NONE:
      return "None (Plain)";
    case SmtpEncryption.SMTP_ENCRYPTION_TLS:
      return "TLS/SSL";
    // Server falls back to starttls.
    case SmtpEncryption.SMTP_ENCRYPTION_STARTTLS:
    case SmtpEncryption.SMTP_ENCRYPTION_UNDEFINED:
    default:
      return "STARTTLS (Default)";
  }
}

const flexColStyle = "flex flex-col gap-2";
