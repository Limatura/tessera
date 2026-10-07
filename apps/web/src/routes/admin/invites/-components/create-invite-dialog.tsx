import { IconCheck, IconCopy } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@tessera/ui/components/dialog";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { useState } from "react";
import type { FormEvent } from "react";

import { useCreateInvite } from "@/hooks/admin/use-admin";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

type ErrorBody = components["schemas"]["ErrorBody"];

const DEFAULT_USES = "1";
const DEFAULT_EXPIRES_IN_DAYS = "7";

export function CreateInviteDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    // 點到外面不關：邀請碼只顯示這一次，手滑關掉就再也拿不回來。
    <Dialog disablePointerDismissal onOpenChange={onOpenChange} open={open}>
      <DialogContent>
        {/* 每次打開重新掛載，上一次的邀請碼不會留在畫面上。 */}
        {open && <CreateInviteFlow onDone={() => onOpenChange(false)} />}
      </DialogContent>
    </Dialog>
  );
}

function CreateInviteFlow({ onDone }: { onDone: () => void }) {
  const createInvite = useCreateInvite();

  if (createInvite.data) {
    return <CreatedInvite code={createInvite.data.code} onDone={onDone} />;
  }
  return <CreateInviteForm mutation={createInvite} onCancel={onDone} />;
}

function CreateInviteForm({
  mutation,
  onCancel,
}: {
  mutation: ReturnType<typeof useCreateInvite>;
  onCancel: () => void;
}) {
  const [uses, setUses] = useState(DEFAULT_USES);
  const [expiresInDays, setExpiresInDays] = useState(DEFAULT_EXPIRES_IN_DAYS);
  const [note, setNote] = useState("");
  const error = mutation.error as Partial<ErrorBody> | null;

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const trimmedNote = note.trim();
    mutation.mutate({
      // 留白代表永不過期。
      expires_in_days:
        expiresInDays.trim() === "" ? null : Number(expiresInDays),
      note: trimmedNote === "" ? null : trimmedNote,
      uses: Number(uses),
    });
  };

  return (
    <>
      <DialogHeader>
        <DialogTitle>{m.invites_create_title()}</DialogTitle>
        <DialogDescription>{m.invites_create_description()}</DialogDescription>
      </DialogHeader>

      <form id="create-invite-form" onSubmit={handleSubmit}>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="invite-note">
              {m.invites_create_note_label()}
            </FieldLabel>
            <Input
              id="invite-note"
              onChange={(event) => setNote(event.target.value)}
              placeholder={m.invites_create_note_placeholder()}
              value={note}
            />
          </Field>

          <div className="grid gap-4 sm:grid-cols-2">
            <Field>
              <FieldLabel htmlFor="invite-uses">
                {m.invites_create_uses_label()}
              </FieldLabel>
              <Input
                id="invite-uses"
                inputMode="numeric"
                min={1}
                onChange={(event) => setUses(event.target.value)}
                required
                step={1}
                type="number"
                value={uses}
              />
              <FieldDescription>
                {m.invites_create_uses_description()}
              </FieldDescription>
            </Field>

            <Field>
              <FieldLabel htmlFor="invite-expires">
                {m.invites_create_expires_label()}
              </FieldLabel>
              <Input
                id="invite-expires"
                inputMode="numeric"
                min={1}
                onChange={(event) => setExpiresInDays(event.target.value)}
                placeholder={m.common_never_expires()}
                step={1}
                type="number"
                value={expiresInDays}
              />
              <FieldDescription>
                {m.invites_create_expires_description()}
              </FieldDescription>
            </Field>
          </div>
        </FieldGroup>
      </form>

      {error && (
        <p className="text-destructive text-sm" role="alert">
          {apiErrorMessage(error, m.invites_create_error())}
        </p>
      )}

      <DialogFooter>
        <Button onClick={onCancel} type="button" variant="outline">
          {m.common_cancel()}
        </Button>
        <Button
          disabled={mutation.isPending}
          form="create-invite-form"
          type="submit"
        >
          {mutation.isPending
            ? m.invites_create_submitting()
            : m.invites_create_submit()}
        </Button>
      </DialogFooter>
    </>
  );
}

function CreatedInvite({ code, onDone }: { code: string; onDone: () => void }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // 不是 https、或使用者拒絕了剪貼簿權限。邀請碼本身還在畫面上，選取複製就行。
    }
  };

  return (
    <>
      <DialogHeader>
        <DialogTitle>{m.invites_created_title()}</DialogTitle>
        <DialogDescription>{m.invites_created_description()}</DialogDescription>
      </DialogHeader>

      <div className="flex items-center gap-2">
        <code className="min-w-0 flex-1 select-all break-all rounded-md bg-muted px-3 py-2 font-mono text-sm">
          {code}
        </code>
        <Button onClick={handleCopy} size="sm" type="button" variant="outline">
          {copied ? <IconCheck size={16} /> : <IconCopy size={16} />}
          {copied ? m.invites_created_copied() : m.invites_created_copy()}
        </Button>
      </div>

      <DialogFooter>
        <Button onClick={onDone} type="button">
          {m.invites_created_done()}
        </Button>
      </DialogFooter>
    </>
  );
}
