import type { components } from "@tessera/api/schema";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { Textarea } from "@tessera/ui/components/textarea";

import type {
  LocationFields,
  LocationKind,
} from "@/lib/admin/backend-location";
import { LOCATION_KINDS } from "@/lib/admin/backend-location";
import * as m from "@/paraglide/messages";

import type { useStorageBackendForm } from "./use-storage-backend-form";

type Backend = components["schemas"]["BackendResponse"];
type StorageBackendForm = ReturnType<typeof useStorageBackendForm>;
type TextKey = Exclude<keyof LocationFields, "kind">;

/** Read at call time so a language switch applies immediately. */
function kindLabels(): Record<LocationKind, string> {
  return {
    azure: m.settings_storage_backends_kind_azure(),
    gcs: m.settings_storage_backends_kind_gcs(),
    local: m.settings_storage_backends_kind_local(),
    other: m.settings_storage_backends_kind_other(),
    s3: m.settings_storage_backends_kind_s3(),
    webdav: m.settings_storage_backends_kind_webdav(),
  };
}

/**
 * Where a backend is, asked for by kind: a bucket and endpoint for S3, a path
 * for a local directory, and so on, instead of one URL the operator has to
 * know the shape of. Secrets start blank; blank keeps the stored one.
 */
export function BackendLocationFields({
  backend,
  form,
}: {
  backend: Backend | null;
  form: StorageBackendForm;
}) {
  const labels = kindLabels();
  const { kind } = form.location;
  const keep = backend !== null;

  return (
    <>
      <Field>
        <FieldLabel htmlFor="backend-kind">
          {m.settings_storage_backends_kind_label()}
        </FieldLabel>
        <Select
          onValueChange={(next) =>
            form.setLocationField("kind", next as LocationKind)
          }
          value={kind}
        >
          <SelectTrigger className="w-full" id="backend-kind">
            <SelectValue>{(value: LocationKind) => labels[value]}</SelectValue>
          </SelectTrigger>
          <SelectContent alignItemWithTrigger={false}>
            {LOCATION_KINDS.map((option) => (
              <SelectItem key={option} value={option}>
                {labels[option]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>

      {kind === "local" && (
        <LocationInput
          description={m.settings_storage_backends_path_description()}
          field="path"
          form={form}
          label={m.settings_storage_backends_path_label()}
          placeholder="/var/lib/tessera/blobs"
        />
      )}

      {kind === "s3" && (
        <>
          <BucketFields form={form} />
          <LocationInput
            description={m.settings_storage_backends_endpoint_description()}
            field="endpoint"
            form={form}
            label={m.settings_storage_backends_endpoint_label()}
            placeholder="http://garage:3900"
          />
          <LocationInput
            description={m.settings_storage_backends_region_description()}
            field="region"
            form={form}
            label={m.settings_storage_backends_region_label()}
            placeholder="us-east-1"
          />
          <LocationInput
            description={keepDescription(keep, backend?.access_key)}
            field="accessKeyId"
            form={form}
            label={m.settings_storage_backends_access_key_label()}
            placeholder={
              keep ? m.settings_storage_backends_keep_placeholder() : ""
            }
          />
          <LocationInput
            field="secretAccessKey"
            form={form}
            label={m.settings_storage_backends_secret_key_label()}
            placeholder={
              keep ? m.settings_storage_backends_keep_placeholder() : ""
            }
            secret
          />
        </>
      )}

      {kind === "gcs" && (
        <>
          <BucketFields form={form} />
          <LocationInput
            description={m.settings_storage_backends_service_account_description()}
            field="serviceAccountKey"
            form={form}
            label={m.settings_storage_backends_service_account_label()}
            multiline
            placeholder={
              keep
                ? m.settings_storage_backends_keep_placeholder()
                : '{"type": "service_account", ...}'
            }
          />
        </>
      )}

      {kind === "azure" && (
        <>
          <BucketFields container form={form} />
          <LocationInput
            field="account"
            form={form}
            label={m.settings_storage_backends_account_label()}
            placeholder="mystorageaccount"
          />
          <LocationInput
            description={m.settings_storage_backends_account_key_description()}
            field="accountKey"
            form={form}
            label={m.settings_storage_backends_account_key_label()}
            placeholder={
              keep ? m.settings_storage_backends_keep_placeholder() : ""
            }
            secret
          />
        </>
      )}

      {kind === "webdav" && (
        <>
          <LocationInput
            field="url"
            form={form}
            label={m.settings_storage_backends_editor_url_label()}
            placeholder="https://dav.example.com/tessera"
          />
          <LocationInput
            description={keepDescription(keep, backend?.access_key)}
            field="username"
            form={form}
            label={m.settings_storage_backends_username_label()}
            placeholder={
              keep ? m.settings_storage_backends_keep_placeholder() : ""
            }
          />
          <LocationInput
            field="password"
            form={form}
            label={m.settings_storage_backends_password_label()}
            placeholder={
              keep ? m.settings_storage_backends_keep_placeholder() : ""
            }
            secret
          />
        </>
      )}

      {kind === "other" && (
        <LocationInput
          description={m.settings_storage_backends_other_description()}
          field="url"
          form={form}
          label={m.settings_storage_backends_editor_url_label()}
          placeholder="memory://"
        />
      )}
    </>
  );
}

/** "Current: AKIA…MPLE. Leave blank to keep it." on an edit; nothing on an add. */
function keepDescription(
  keep: boolean,
  current: string | null | undefined
): string | undefined {
  if (!keep) {
    return undefined;
  }
  return m.settings_storage_backends_keep_description({
    key: current ?? m.settings_storage_backends_editor_url_no_key(),
  });
}

function BucketFields({
  form,
  container = false,
}: {
  form: StorageBackendForm;
  container?: boolean;
}) {
  return (
    <div className="grid gap-4 sm:grid-cols-2">
      <LocationInput
        field="bucket"
        form={form}
        label={
          container
            ? m.settings_storage_backends_container_label()
            : m.settings_storage_backends_bucket_label()
        }
        placeholder="tessera"
      />
      <LocationInput
        field="prefix"
        form={form}
        label={m.settings_storage_backends_prefix_label()}
        placeholder="uploads"
      />
    </div>
  );
}

function LocationInput({
  form,
  field,
  label,
  placeholder,
  description,
  secret = false,
  multiline = false,
}: {
  form: StorageBackendForm;
  field: TextKey;
  label: string;
  placeholder?: string;
  description?: string;
  secret?: boolean;
  multiline?: boolean;
}) {
  const id = `backend-${field}`;
  const problem = form.problems[field];
  const shared = {
    "aria-invalid": problem !== undefined,
    autoComplete: secret ? "new-password" : "off",
    id,
    placeholder,
    value: form.location[field],
  };

  return (
    <Field>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      {multiline ? (
        <Textarea
          {...shared}
          className="font-mono text-xs"
          onChange={(event) => form.setLocationField(field, event.target.value)}
          rows={3}
        />
      ) : (
        <Input
          {...shared}
          onChange={(event) => form.setLocationField(field, event.target.value)}
          type={secret ? "password" : "text"}
        />
      )}
      {problem ? (
        <FieldError>{problem}</FieldError>
      ) : (
        description && <FieldDescription>{description}</FieldDescription>
      )}
    </Field>
  );
}
