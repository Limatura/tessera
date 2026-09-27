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
import { Switch } from "@tessera/ui/components/switch";

import * as m from "@/paraglide/messages";

import { useStorageBackendForm } from "./use-storage-backend-form";

type Backend = components["schemas"]["BackendResponse"];
type StorageBackendForm = ReturnType<typeof useStorageBackendForm>;

/** 新增或覆蓋一列 backend。 */
export function StorageBackendEditorDialog({
  backend,
  open,
  onOpenChange,
}: {
  /** `null` 代表新增。 */
  backend: Backend | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const close = () => onOpenChange(false);

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="sm:max-w-lg">
        {/* key 換 backend 時重新掛載表單，初始值才會跟著換。 */}
        {open && (
          <StorageBackendEditorForm
            backend={backend}
            key={backend?.name ?? "new"}
            onCancel={close}
            onDone={close}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function StorageBackendEditorForm({
  backend,
  onDone,
  onCancel,
}: {
  backend: Backend | null;
  onDone: () => void;
  onCancel: () => void;
}) {
  const form = useStorageBackendForm({ backend, onDone });

  return (
    <>
      <DialogHeader>
        <DialogTitle>
          {backend
            ? m.settings_storage_backends_editor_edit_title({
                name: backend.name,
              })
            : m.settings_storage_backends_editor_add_title()}
        </DialogTitle>
        <DialogDescription>
          {backend
            ? m.settings_storage_backends_editor_description_edit()
            : m.settings_storage_backends_editor_description_add()}
        </DialogDescription>
      </DialogHeader>

      <form id="storage-backend-editor-form" onSubmit={form.submit}>
        <FieldGroup>
          <NameField backend={backend} form={form} />
          <UrlField form={form} />
          <CapacityFields form={form} />
          <ToggleFields form={form} />

          {form.error && (
            <p className="text-destructive text-sm" role="alert">
              {form.error.message ??
                m.settings_storage_backends_editor_save_error()}
            </p>
          )}
        </FieldGroup>
      </form>

      <DialogFooter>
        <Button onClick={onCancel} type="button" variant="outline">
          {m.common_cancel()}
        </Button>
        <Button
          disabled={form.pending || !form.canSubmit}
          form="storage-backend-editor-form"
          type="submit"
        >
          {form.pending ? m.common_saving() : m.common_save()}
        </Button>
      </DialogFooter>
    </>
  );
}

function NameField({
  backend,
  form,
}: {
  backend: Backend | null;
  form: StorageBackendForm;
}) {
  return (
    <Field>
      <FieldLabel htmlFor="backend-name">
        {m.settings_storage_backends_editor_name_label()}
      </FieldLabel>
      <Input
        disabled={backend !== null}
        id="backend-name"
        onChange={(event) => form.setName(event.target.value)}
        placeholder={m.settings_storage_backends_editor_name_placeholder()}
        required
        value={form.name}
      />
      <FieldDescription>
        {backend
          ? m.settings_storage_backends_editor_name_description_edit()
          : m.settings_storage_backends_editor_name_description_add()}
      </FieldDescription>
    </Field>
  );
}

function UrlField({ form }: { form: StorageBackendForm }) {
  return (
    <Field>
      <FieldLabel htmlFor="backend-url">
        {m.settings_storage_backends_editor_url_label()}
      </FieldLabel>
      <Input
        autoComplete="off"
        id="backend-url"
        onChange={(event) => form.setUrl(event.target.value)}
        placeholder="s3://key:secret@bucket/prefix"
        required
        value={form.url}
      />
      <FieldDescription>
        {m.settings_storage_backends_editor_url_description()}
      </FieldDescription>
    </Field>
  );
}

function CapacityFields({ form }: { form: StorageBackendForm }) {
  return (
    <>
      <Field orientation="horizontal">
        <FieldLabel htmlFor="backend-capacity-unlimited">
          {m.settings_storage_backends_editor_capacity_unlimited_label()}
        </FieldLabel>
        <Switch
          checked={form.unlimited}
          id="backend-capacity-unlimited"
          onCheckedChange={form.setUnlimited}
        />
      </Field>

      {!form.unlimited && (
        <Field>
          <FieldLabel htmlFor="backend-capacity">
            {m.settings_storage_backends_editor_capacity_label()}
          </FieldLabel>
          <Input
            id="backend-capacity"
            min={0}
            onChange={(event) =>
              form.setCapacityGib(event.target.valueAsNumber)
            }
            type="number"
            value={Number.isNaN(form.capacityGib) ? "" : form.capacityGib}
          />
          <FieldDescription>
            {m.settings_storage_backends_editor_capacity_description()}
          </FieldDescription>
        </Field>
      )}
    </>
  );
}

function ToggleFields({ form }: { form: StorageBackendForm }) {
  return (
    <>
      <Field orientation="horizontal">
        <FieldLabel htmlFor="backend-direct-access">
          {m.settings_storage_backends_editor_direct_access_label()}
          <FieldDescription>
            {m.settings_storage_backends_editor_direct_access_description()}
          </FieldDescription>
        </FieldLabel>
        <Switch
          checked={form.directAccess}
          id="backend-direct-access"
          onCheckedChange={form.setDirectAccess}
        />
      </Field>

      <Field orientation="horizontal">
        <FieldLabel htmlFor="backend-read-only">
          {m.settings_storage_backends_editor_read_only_label()}
          <FieldDescription>
            {m.settings_storage_backends_editor_read_only_description()}
          </FieldDescription>
        </FieldLabel>
        <Switch
          checked={form.readOnly}
          id="backend-read-only"
          onCheckedChange={form.setReadOnly}
        />
      </Field>
    </>
  );
}
