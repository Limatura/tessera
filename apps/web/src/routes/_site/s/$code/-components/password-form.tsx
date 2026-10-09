import { Button } from "@tessera/ui/components/button";
import { Field, FieldError, FieldGroup } from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { Eye, EyeOff } from "lucide-react";
import { useState } from "react";

import * as m from "@/paraglide/messages";

/**
 * 分享密碼的長度不固定(後端只規定至少 4 碼),所以用一般的密碼輸入框,
 * 不用固定格數的 OTP 輸入(#357)。
 */
export function PasswordForm({
  error,
  onSubmit,
  pending,
}: {
  error: boolean;
  onSubmit: (password: string) => void;
  pending: boolean;
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
          {pending ? m.share_unlocking() : m.share_unlock()}
        </Button>
      </FieldGroup>
    </form>
  );
}
