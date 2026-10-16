import type { components } from "@tessera/api/schema";
import { toast } from "@tessera/ui/components/toast";
import { useState } from "react";
import type * as React from "react";

import type { ByteLimitMode } from "@/components/admin/byte-limit-field";
import { toBytes, toMib } from "@/components/admin/byte-limit-field";
import {
  useAddStorageBackend,
  useReplaceStorageBackend,
  useStorageBackends,
} from "@/hooks/admin/use-storage-backends";
import type { LocationFields } from "@/lib/admin/backend-location";
import {
  endpointIsValid,
  halfPairedField,
  locationFieldsOf,
  locationRequestOf,
  missingLocationFields,
  movesLocation,
} from "@/lib/admin/backend-location";
import { backendUrlProblem } from "@/lib/admin/storage-backend-url";
import * as m from "@/paraglide/messages";

type Backend = components["schemas"]["BackendResponse"];
type ErrorBody = components["schemas"]["ErrorBody"];
type CapacityMode = Exclude<ByteLimitMode, "inherit">;
export type LocationProblems = Partial<Record<keyof LocationFields, string>>;

/**
 * Form state for adding or overwriting a backend row.
 *
 * The location is asked for by kind (bucket, endpoint, keys...) and put
 * together into the URL and options the server takes; see
 * `lib/admin/backend-location.ts`. Secrets start blank and blank keeps the
 * stored ones, as long as the backend stays on the same store; the server
 * refuses one that moved elsewhere (`backend_credentials_required`).
 */
export function useStorageBackendForm({
  backend,
  onDone,
}: {
  backend: Backend | null;
  onDone: () => void;
}) {
  const addBackend = useAddStorageBackend();
  const replaceBackend = useReplaceStorageBackend();
  const save = backend ? replaceBackend : addBackend;
  const error = save.error as ErrorBody | null;
  const { data: existing } = useStorageBackends();

  const [name, setName] = useState(backend?.name ?? "");
  const [location, setLocation] = useState(() => locationFieldsOf(backend));
  // No level above a backend to follow, so only unlimited or a number; the
  // server reads `null` and `0` alike as no limit.
  const [capacityMode, setCapacityMode] = useState<CapacityMode>(
    backend?.capacity_bytes ? "custom" : "unlimited"
  );
  const [capacityMib, setCapacityMib] = useState(
    toMib(backend?.capacity_bytes)
  );
  const [directAccess, setDirectAccess] = useState(
    backend?.direct_access ?? false
  );
  const [readOnly, setReadOnly] = useState(backend?.read_only ?? false);

  const setLocationField = <K extends keyof LocationFields>(
    key: K,
    value: LocationFields[K]
  ) => setLocation((current) => ({ ...current, [key]: value }));

  const problems = locationProblems(location);

  // Files already written stay at the old location, so moving one that holds
  // some is worth a warning before it breaks their downloads.
  const movesFiles =
    backend !== null &&
    backend.used_bytes > 0 &&
    movesLocation(backend, location);

  // Names compare as typed: the server's primary key does, too.
  const nameTaken =
    backend === null &&
    (existing ?? []).some((other) => other.name === name.trim());

  // The server saves a backend it cannot reach, so a wrong address is not an
  // error; say so here, since the dialog is about to close on a green save.
  const done = (saved: Backend) => {
    if (!saved.reachable) {
      toast.add({
        description: saved.last_error ?? undefined,
        title: m.settings_storage_backends_saved_unreachable({
          name: saved.name,
        }),
        type: "warning",
      });
    }
    onDone();
  };

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    const body = {
      ...locationRequestOf(location),
      capacity_bytes:
        capacityMode === "custom" ? toBytes("custom", capacityMib) : null,
      direct_access: directAccess,
      read_only: readOnly,
    };
    if (backend) {
      replaceBackend.mutate({ body, name: backend.name }, { onSuccess: done });
      return;
    }
    addBackend.mutate(
      { ...body, name: name.trim(), options: body.options ?? {} },
      { onSuccess: done }
    );
  };

  return {
    capacityMib,
    capacityMode,
    canSubmit:
      name.trim() !== "" &&
      !nameTaken &&
      missingLocationFields(location).length === 0 &&
      Object.keys(problems).length === 0,
    directAccess,
    error,
    location,
    movesFiles,
    name,
    nameTaken,
    pending: save.isPending,
    problems,
    readOnly,
    setCapacityMib,
    setCapacityMode,
    setDirectAccess,
    setLocationField,
    setName,
    setReadOnly,
    submit,
  };
}

/**
 * What is wrong with the location, by the field to show it under. Blank
 * fields are not problems here: the submit button waits for them instead, so
 * an empty new form is not red on arrival.
 */
function locationProblems(fields: LocationFields): LocationProblems {
  const problems: LocationProblems = {};
  const { url } = locationRequestOf(fields);
  const urlField = urlFieldOf(fields);
  const urlProblem =
    String(fields[urlField]).trim() === "" ? null : backendUrlProblem(url);
  if (urlProblem) {
    problems[urlField] = urlProblem;
  }
  if (fields.kind === "s3" && !endpointIsValid(fields.endpoint)) {
    problems.endpoint = m.settings_storage_backends_endpoint_invalid();
  }
  const half = halfPairedField(fields);
  if (half) {
    problems[half] = m.settings_storage_backends_pair_incomplete();
  }
  return problems;
}

/** The field a URL problem belongs under for each kind. */
function urlFieldOf(fields: LocationFields): keyof LocationFields {
  switch (fields.kind) {
    case "local": {
      return "path";
    }
    case "webdav":
    case "other": {
      return "url";
    }
    default: {
      return "bucket";
    }
  }
}
