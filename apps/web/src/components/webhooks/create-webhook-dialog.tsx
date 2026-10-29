import { IconCheck, IconCopy } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import { Checkbox } from "@tessera/ui/components/checkbox";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { useState } from "react";
import type { FormEvent } from "react";

import { apiErrorMessage } from "@/lib/api-error";
import { copyText } from "@/lib/copy-text";
import * as m from "@/paraglide/messages";

import { eventLabel, WEBHOOK_EVENTS } from "./labels";
import type { WebhooksSource } from "./types";
import { useCreateWebhook } from "./use-webhooks";

const COPIED_FEEDBACK_MS = 2000;
const FORMATS = ["json", "discord"] as const;
const MAX_URL_LENGTH = 2048;

function formatLabel(format: string): string {
  return format === "discord"
    ? m.webhooks_format_discord()
    : m.webhooks_format_json();
}

export function CreateWebhookDialog({
  source,
  open,
  onOpenChange,
}: {
  source: WebhooksSource;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    // Clicking outside does not close it: the secret is shown only this once,
    // and a slip would lose it.
    <ResponsiveDialog
      disablePointerDismissal
      onOpenChange={onOpenChange}
      open={open}
    >
      <ResponsiveDialogContent>
        {/* Remounted on every open, so the previous secret does not linger. */}
        {open && (
          <CreateFlow source={source} onDone={() => onOpenChange(false)} />
        )}
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}

function CreateFlow({
  source,
  onDone,
}: {
  source: WebhooksSource;
  onDone: () => void;
}) {
  const create = useCreateWebhook(source);

  if (create.data) {
    return <SecretStep onDone={onDone} secret={create.data.secret} />;
  }
  return <CreateForm mutation={create} onCancel={onDone} />;
}

function CreateForm({
  mutation,
  onCancel,
}: {
  mutation: ReturnType<typeof useCreateWebhook>;
  onCancel: () => void;
}) {
  const [url, setUrl] = useState("");
  const [events, setEvents] = useState<Set<string>>(new Set(["share.created"]));
  const [format, setFormat] = useState<string>("json");
  const trimmed = url.trim();

  const handleToggle = (event: string, checked: boolean) => {
    setEvents((current) => {
      const next = new Set(current);
      if (checked) {
        next.add(event);
      } else {
        next.delete(event);
      }
      return next;
    });
  };

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    mutation.mutate({ events: [...events], format, url: trimmed });
  };

  return (
    <form className="contents" onSubmit={handleSubmit}>
      <ResponsiveDialogHeader>
        <ResponsiveDialogTitle>
          {m.webhooks_create_title()}
        </ResponsiveDialogTitle>
        <ResponsiveDialogDescription>
          {m.webhooks_create_description()}
        </ResponsiveDialogDescription>
      </ResponsiveDialogHeader>
      <div className="space-y-4">
        <Field>
          <FieldLabel htmlFor="webhook-url">{m.webhooks_url()}</FieldLabel>
          <Input
            // oxlint-disable-next-line jsx-a11y/no-autofocus -- first input in the dialog
            autoFocus
            id="webhook-url"
            inputMode="url"
            maxLength={MAX_URL_LENGTH}
            onChange={(event) => setUrl(event.target.value)}
            placeholder="https://example.com/hooks/tessera"
            type="url"
            value={url}
          />
        </Field>
        <fieldset className="space-y-2">
          <legend className="font-medium text-sm">{m.webhooks_events()}</legend>
          {WEBHOOK_EVENTS.map((event) => (
            // oxlint-disable-next-line jsx-a11y/label-has-associated-control -- Checkbox is a base-ui button, so wrapping it in the label associates them
            <label
              className="flex cursor-pointer items-center gap-3 text-sm"
              key={event}
            >
              <Checkbox
                checked={events.has(event)}
                onCheckedChange={(checked) => handleToggle(event, checked)}
              />
              {eventLabel(event)}
            </label>
          ))}
        </fieldset>
        <Field>
          <FieldLabel htmlFor="webhook-format">
            {m.webhooks_format()}
          </FieldLabel>
          <Select
            onValueChange={(value) => setFormat(value as string)}
            value={format}
          >
            <SelectTrigger id="webhook-format">
              <SelectValue>{(value: string) => formatLabel(value)}</SelectValue>
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {FORMATS.map((choice) => (
                  <SelectItem key={choice} value={choice}>
                    {formatLabel(choice)}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
          <FieldDescription>{m.webhooks_format_hint()}</FieldDescription>
        </Field>
        {mutation.error ? (
          <FieldError>
            {apiErrorMessage(mutation.error, m.common_try_again())}
          </FieldError>
        ) : null}
      </div>
      <ResponsiveDialogFooter>
        <Button onClick={onCancel} type="button" variant="outline">
          {m.common_cancel()}
        </Button>
        <Button
          disabled={mutation.isPending || trimmed === "" || events.size === 0}
          type="submit"
        >
          {mutation.isPending ? m.webhooks_adding() : m.webhooks_add()}
        </Button>
      </ResponsiveDialogFooter>
    </form>
  );
}

function SecretStep({
  secret,
  onDone,
}: {
  secret: string;
  onDone: () => void;
}) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    if (!(await copyText(secret))) {
      return;
    }
    setCopied(true);
    setTimeout(() => setCopied(false), COPIED_FEEDBACK_MS);
  };

  return (
    <>
      <ResponsiveDialogHeader>
        <ResponsiveDialogTitle>
          {m.webhooks_secret_title()}
        </ResponsiveDialogTitle>
        <ResponsiveDialogDescription>
          {m.webhooks_secret_warning()}
        </ResponsiveDialogDescription>
      </ResponsiveDialogHeader>
      <code className="block break-all rounded-lg border bg-muted/40 p-3 font-mono text-sm">
        {secret}
      </code>
      <ResponsiveDialogFooter>
        <Button onClick={handleCopy} type="button" variant="outline">
          {copied ? <IconCheck size={16} /> : <IconCopy size={16} />}
          {copied ? m.webhooks_copied() : m.webhooks_copy()}
        </Button>
        <Button onClick={onDone} type="button">
          {m.webhooks_done()}
        </Button>
      </ResponsiveDialogFooter>
    </>
  );
}
