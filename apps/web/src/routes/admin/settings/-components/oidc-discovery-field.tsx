import {
  IconAlertCircle,
  IconCircleCheck,
  IconLoader2,
} from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import {
  Field,
  FieldDescription,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { useState } from "react";

import type { ReadResult } from "@/lib/admin/oidc-discovery";
import { readDiscovery } from "@/lib/admin/oidc-discovery";
import * as m from "@/paraglide/messages";

/**
 * Paste a discovery URL, have the server read the document once, and fill the fields
 * below from it.
 *
 * Values are read, not derived from the URL: the document's `issuer` is authoritative
 * and the spec requires it to match the `iss` claim character for character. Trimming
 * the URL suffix usually gives the same answer, but it is a guess.
 *
 * The backend fetches it (see `lib/admin/oidc-discovery.ts`): many providers refuse
 * cross-origin reads from browsers, and if the server can fetch it, so can sign-in.
 *
 * Only issuer and scopes are filled in: the backend reads the endpoints from the same
 * document itself (see `discovery.rs`), and no field stores them. Client ID and secret
 * are issued separately in the provider's console and are not in the document.
 */
export function OidcDiscoveryField({
  issuer,
  scopes,
  onApply,
}: {
  issuer: string;
  scopes: string;
  onApply: (values: {
    issuer: string;
    scopes?: string;
    backchannelLogout: boolean;
    claimsSupported: string[] | null;
  }) => void;
}) {
  const [value, setValue] = useState("");
  const [result, setResult] = useState<ReadResult | null>(null);
  const [isReading, setIsReading] = useState(false);

  // Empty falls back to the Issuer below: a configured instance is here to
  // re-check, and should not have to paste it again.
  const target = value.trim() === "" ? issuer.trim() : value.trim();

  const handleRead = async () => {
    if (target === "" || isReading) {
      return;
    }
    setIsReading(true);
    const read = await readDiscovery(
      target,
      scopes.split(/\s+/u).filter(Boolean)
    );
    setIsReading(false);
    setResult(read);

    if (read.ok) {
      onApply({
        backchannelLogout: read.backchannelLogout,
        claimsSupported: read.claimsSupported,
        issuer: read.issuer,
        // Without scopes_supported, leave the user's scopes alone: undeclared is not unsupported.
        scopes: read.scopes ? read.scopes.join(" ") : undefined,
      });
    }
  };

  return (
    <div className="space-y-3 rounded-md border border-dashed p-3">
      <Field>
        <FieldLabel htmlFor="oidc-discovery-url">
          {m.settings_oidc_discovery_field_url_label()}
        </FieldLabel>
        <div className="flex flex-col gap-2 sm:flex-row">
          <Input
            autoComplete="off"
            className="sm:flex-1"
            id="oidc-discovery-url"
            onChange={(event) => setValue(event.target.value)}
            placeholder="https://auth.example.com/.well-known/openid-configuration"
            value={value}
          />
          <Button
            className="sm:w-auto"
            disabled={target === "" || isReading}
            onClick={handleRead}
            type="button"
            variant="outline"
          >
            {isReading ? (
              <IconLoader2 className="animate-spin" size={16} />
            ) : null}
            {m.settings_oidc_discovery_field_read_button()}
          </Button>
        </div>
        <FieldDescription>
          {m.settings_oidc_discovery_field_description()}
        </FieldDescription>
      </Field>

      {result ? <ReadMessage result={result} /> : null}
    </div>
  );
}

function ReadMessage({ result }: { result: ReadResult }) {
  if (!result.ok) {
    return (
      <div
        className="flex gap-2 rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm"
        role="alert"
      >
        <IconAlertCircle
          className="mt-0.5 size-4 shrink-0 text-destructive"
          size={16}
        />
        <p>{result.reason}</p>
      </div>
    );
  }

  const missingEmail =
    result.supportedScopes !== null &&
    !result.supportedScopes.includes("email");

  return (
    <output className="flex gap-2 rounded-md border border-emerald-500/40 bg-emerald-500/10 p-3 text-sm">
      <IconCircleCheck
        className="mt-0.5 size-4 shrink-0 text-emerald-600 dark:text-emerald-500"
        size={16}
      />
      <div className="space-y-1">
        <p className="font-medium">
          {m.settings_oidc_discovery_field_read_success()}
        </p>
        {result.supportedScopes ? (
          <p className="text-muted-foreground">
            {m.settings_oidc_discovery_field_supported_scopes({
              scopes: result.supportedScopes.join("、"),
            })}
          </p>
        ) : (
          <p className="text-muted-foreground">
            {m.settings_oidc_discovery_field_no_scopes_declared()}
          </p>
        )}
        {/* Without an email there is no account to create, and the backend refuses every
            sign-in with no_address. Better said now than found at the first failed sign-in. */}
        {missingEmail ? (
          <p className="font-medium text-amber-600 dark:text-amber-500">
            {m.settings_oidc_discovery_field_missing_email()}
          </p>
        ) : null}
      </div>
    </output>
  );
}
