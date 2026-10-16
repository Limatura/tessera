import { MIN_PASSWORD_LENGTH } from "@tessera/crypto";
import { Input } from "@tessera/ui/components/input";
import { Label } from "@tessera/ui/components/label";
import { Switch } from "@tessera/ui/components/switch";
import { Eye, EyeOff, KeyRound, LockKeyhole } from "lucide-react";
import { useState } from "react";

import type { EncryptionChoice } from "@/lib/upload/encryption";
import * as m from "@/paraglide/messages";

export interface EncryptionState {
  enabled: boolean;
  usePassword: boolean;
  password: string;
}

export const NO_ENCRYPTION: EncryptionState = {
  enabled: false,
  password: "",
  usePassword: false,
};

/**
 * What an upload should do with this state: `undefined` for a plain share,
 * `null` when the password is too short to use. Counted in characters, not
 * UTF-16 units, the same as the crate's `MIN_PASSWORD_CHARS`.
 */
export function encryptionChoice(
  state: EncryptionState
): EncryptionChoice | null | undefined {
  if (!state.enabled) {
    return;
  }
  if (!state.usePassword) {
    return { mode: "link" };
  }
  return [...state.password].length >= MIN_PASSWORD_LENGTH
    ? { mode: "password", password: state.password }
    : null;
}

/**
 * The end-to-end encryption switches on the upload card. Decided before the
 * upload, unlike expiry and password, since the bytes are sealed on the way
 * out. The server never sees the password, so its length is checked here.
 */
export function EncryptionField({
  showError,
  state,
  onChange,
}: {
  showError: boolean;
  state: EncryptionState;
  onChange: (next: EncryptionState) => void;
}) {
  const [showPassword, setShowPassword] = useState(false);
  const tooShort =
    state.enabled &&
    state.usePassword &&
    [...state.password].length < MIN_PASSWORD_LENGTH;

  return (
    <div className="flex flex-col gap-3 border-border/60 border-t pt-3">
      <div className="flex items-center justify-between gap-2">
        <Label htmlFor="upload-encrypt">
          <LockKeyhole className="size-3.5 text-green-500" />
          {m.upload_encrypt()}
        </Label>
        <Switch
          checked={state.enabled}
          id="upload-encrypt"
          onCheckedChange={(enabled) => onChange({ ...state, enabled })}
        />
      </div>

      {state.enabled && (
        <>
          <p className="text-muted-foreground text-xs">
            {state.usePassword
              ? m.upload_encrypt_password_hint()
              : m.upload_encrypt_link_hint()}
          </p>
          <div className="flex items-center justify-between gap-2">
            <Label htmlFor="upload-encrypt-password">
              <KeyRound className="size-3.5 text-amber-500" />
              {m.upload_encrypt_use_password()}
            </Label>
            <Switch
              checked={state.usePassword}
              id="upload-encrypt-password"
              onCheckedChange={(usePassword) =>
                onChange({ ...state, usePassword })
              }
            />
          </div>
        </>
      )}

      {state.enabled && state.usePassword && (
        <div className="flex flex-col gap-1.5">
          <div className="relative">
            <Input
              aria-invalid={showError && tooShort}
              aria-label={m.upload_encrypt_password_placeholder({
                min: MIN_PASSWORD_LENGTH,
              })}
              autoComplete="new-password"
              className="pr-8"
              onChange={(event) =>
                onChange({ ...state, password: event.target.value })
              }
              placeholder={m.upload_encrypt_password_placeholder({
                min: MIN_PASSWORD_LENGTH,
              })}
              type={showPassword ? "text" : "password"}
              value={state.password}
            />
            <button
              aria-label={
                showPassword
                  ? m.upload_result_password_hide()
                  : m.upload_result_password_show()
              }
              className="-translate-y-1/2 absolute top-1/2 right-2 text-current/60 hover:text-current"
              onClick={() => setShowPassword((value) => !value)}
              type="button"
            >
              {showPassword ? (
                <EyeOff className="size-3.5" />
              ) : (
                <Eye className="size-3.5" />
              )}
            </button>
          </div>
          {showError && tooShort && (
            <p className="text-destructive text-xs">
              {m.upload_encrypt_password_too_short({
                min: MIN_PASSWORD_LENGTH,
              })}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
