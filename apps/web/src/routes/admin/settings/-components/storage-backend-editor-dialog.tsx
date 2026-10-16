import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldError,
  FieldGroup,
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
import { Switch } from "@tessera/ui/components/switch";
import { formatBytes } from "@tessera/ui/lib/format";

import { ByteLimitField } from "@/components/admin/byte-limit-field";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

import { BackendLocationFields } from "./backend-location-fields";
import { SettingsNotice } from "./form-fields";
import { useStorageBackendForm } from "./use-storage-backend-form";

type Backend = components["schemas"]["BackendResponse"];
type StorageBackendForm = ReturnType<typeof useStorageBackendForm>;

/** Add a backend row or overwrite one. */
export function StorageBackendEditorDialog({
  backend,
  open,
  onOpenChange,
}: {
  /** `null` means adding a new one. */
  backend: Backend | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const close = () => onOpenChange(false);

  return (
    <ResponsiveDialog onOpenChange={onOpenChange} open={open}>
      <ResponsiveDialogContent className="sm:max-w-lg">
        {/* Keyed so switching backends remounts the form with new initial values. */}
        {open && (
          <StorageBackendEditorForm
            backend={backend}
            key={backend?.name ?? "new"}
            onCancel={close}
            onDone={close}
          />
        )}
      </ResponsiveDialogContent>
    </ResponsiveDialog>
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
      <ResponsiveDialogHeader>
        <ResponsiveDialogTitle>
          {backend
            ? m.settings_storage_backends_editor_edit_title({
                name: backend.name,
              })
            : m.settings_storage_backends_editor_add_title()}
        </ResponsiveDialogTitle>
        <ResponsiveDialogDescription>
          {backend
            ? m.settings_storage_backends_editor_description_edit()
            : m.settings_storage_backends_editor_description_add()}
        </ResponsiveDialogDescription>
      </ResponsiveDialogHeader>

      <form id="storage-backend-editor-form" onSubmit={form.submit}>
        <FieldGroup>
          <NameField backend={backend} form={form} />
          <BackendLocationFields backend={backend} form={form} />
          {form.movesFiles && backend && (
            <SettingsNotice tone="warning">
              {m.settings_storage_backends_editor_moves_files({
                size: formatBytes(backend.used_bytes),
              })}
            </SettingsNotice>
          )}
          <CapacityField form={form} />
          <ToggleFields form={form} />

          {form.error && (
            <p className="text-destructive text-sm" role="alert">
              {apiErrorMessage(
                form.error,
                m.settings_storage_backends_editor_save_error()
              )}
            </p>
          )}
        </FieldGroup>
      </form>

      <ResponsiveDialogFooter>
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
      </ResponsiveDialogFooter>
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
        aria-invalid={form.nameTaken}
        disabled={backend !== null}
        id="backend-name"
        onChange={(event) => form.setName(event.target.value)}
        placeholder={m.settings_storage_backends_editor_name_placeholder()}
        required
        value={form.name}
      />
      {form.nameTaken ? (
        <FieldError>
          {m.settings_storage_backends_editor_name_taken()}
        </FieldError>
      ) : (
        <FieldDescription>
          {backend
            ? m.settings_storage_backends_editor_name_description_edit()
            : m.settings_storage_backends_editor_name_description_add()}
        </FieldDescription>
      )}
    </Field>
  );
}

function CapacityField({ form }: { form: StorageBackendForm }) {
  return (
    <ByteLimitField
      disabled={form.pending}
      customDescription={m.settings_storage_backends_editor_capacity_description()}
      id="backend-capacity"
      label={m.settings_storage_backends_editor_capacity_label_short()}
      mib={form.capacityMib}
      mode={form.capacityMode}
      onMibChange={form.setCapacityMib}
      onModeChange={(mode) => {
        if (mode !== "inherit") {
          form.setCapacityMode(mode);
        }
      }}
      unlimitedDescription={m.settings_storage_backends_editor_capacity_unlimited_description()}
    />
  );
}

function ToggleFields({ form }: { form: StorageBackendForm }) {
  return (
    <>
      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor="backend-direct-access">
            {m.settings_storage_backends_editor_direct_access_label()}
          </FieldLabel>
          <FieldDescription>
            {m.settings_storage_backends_editor_direct_access_description()}
          </FieldDescription>
        </FieldContent>
        <Switch
          checked={form.directAccess}
          id="backend-direct-access"
          onCheckedChange={form.setDirectAccess}
        />
      </Field>

      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor="backend-read-only">
            {m.settings_storage_backends_editor_read_only_label()}
          </FieldLabel>
          <FieldDescription>
            {m.settings_storage_backends_editor_read_only_description()}
          </FieldDescription>
        </FieldContent>
        <Switch
          checked={form.readOnly}
          id="backend-read-only"
          onCheckedChange={form.setReadOnly}
        />
      </Field>
    </>
  );
}
