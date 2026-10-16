import { Button } from "@tessera/ui/components/button";
import { Field, FieldError, FieldGroup } from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { Eye, EyeOff } from "lucide-react";
import { useState } from "react";

import * as m from "@/paraglide/messages";

/**
 * Share passwords have no fixed length (the backend only requires at least 4
 * characters), so this is a normal password input, not a fixed-cell OTP input.
 */
export function PasswordForm({
  error,
  onSubmit,
  pending,
  pendingLabel,
}: {
  error: boolean;
  onSubmit: (password: string) => void;
  pending: boolean;
  /** What the button says while busy, when it is not just unlocking. */
  pendingLabel?: string;
}) {
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    onSubmit(password);
  };

  return (
    <form onSubmit={handleSubmit}>
      <FieldGroup>
        <Field data-invalid={error}>
          <div className="relative">
            <Input
              aria-invalid={error}
              aria-label={m.share_password_placeholder()}
              autoComplete="off"
              autoFocus
              className="pr-8"
              onChange={(event) => setPassword(event.target.value)}
              placeholder={m.share_password_placeholder()}
              type={showPassword ? "text" : "password"}
              value={password}
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
          {error && (
            <FieldError className="text-center">
              {m.share_password_incorrect()}
            </FieldError>
          )}
        </Field>
        <Button
          className="w-full"
          disabled={pending || password.length === 0}
          type="submit"
        >
          {pending ? (pendingLabel ?? m.share_unlocking()) : m.share_unlock()}
        </Button>
      </FieldGroup>
    </form>
  );
}
