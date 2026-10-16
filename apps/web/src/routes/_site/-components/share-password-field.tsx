import { Button } from "@tessera/ui/components/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@tessera/ui/components/collapsible";
import { Input } from "@tessera/ui/components/input";
import { toast } from "@tessera/ui/components/toast";
import { ChevronDown, Eye, EyeOff, Lock } from "lucide-react";
import { useRef, useState } from "react";

import { apiErrorMessage } from "@/lib/api-error";
import type { ShareSettings } from "@/lib/upload/share-api";
import { updateShare } from "@/lib/upload/share-api";
import * as m from "@/paraglide/messages";

interface SharePasswordFieldProps {
  code: string;
  initialHasPassword: boolean;
  managementToken?: string;
  /**
   * Called only after a save succeeds, for callers that track whether a password is set.
   */
  onChange?: (hasPassword: boolean) => void;
}

/**
 * The password section of the share result card: an expandable panel to set,
 * change or remove the password. The typed value stays inside; only whether a
 * password is set is visible outside.
 */
export function SharePasswordField({
  code,
  initialHasPassword,
  managementToken,
  onChange,
}: SharePasswordFieldProps) {
  const [hasPassword, setHasPassword] = useState(initialHasPassword);
  const [open, setOpen] = useState(false);
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [saving, setSaving] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const handleOpenChange = (nextOpen: boolean) => {
    setOpen(nextOpen);
    setPassword("");
    setShowPassword(false);
    if (nextOpen) {
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  };

  /**
   * Submits one password change. A value in `settings.password` sets it; the
   * key present with no value removes it, since `updateShare` tells the two
   * apart by whether the key exists.
   */
  const submit = async (settings: ShareSettings, nextHasPassword: boolean) => {
    setSaving(true);
    try {
      await updateShare(code, settings, managementToken);
      setHasPassword(nextHasPassword);
      onChange?.(nextHasPassword);
      setOpen(false);
      setPassword("");
      setShowPassword(false);
    } catch (error) {
      toast.add({
        description: apiErrorMessage(error, m.common_try_again()),
        title: m.upload_result_update_failed(),
        type: "error",
      });
    } finally {
      setSaving(false);
    }
  };

  const handleSave = () => {
    const trimmed = password.trim();
    // An empty string means no password and takes the same path as removing it.
    if (trimmed.length === 0) {
      handleRemove();
      return;
    }
    submit({ password: trimmed }, true);
  };

  function handleRemove() {
    submit({ password: undefined }, false);
  }

  const label = hasPassword
    ? m.upload_result_password_set()
    : m.upload_result_password_unset();

  return (
    <Collapsible onOpenChange={handleOpenChange} open={open}>
      <CollapsibleTrigger
        className="group flex w-full items-center justify-between gap-2 text-current"
        render={<button aria-label={label} type="button" />}
      >
        <span className="flex items-center gap-2 font-medium text-xs/relaxed">
          <Lock className="size-3.5" />
          {label}
        </span>
        <ChevronDown className="size-3.5 text-current/60 transition-transform group-data-panel-open:rotate-180" />
      </CollapsibleTrigger>

      <CollapsibleContent>
        <div className="flex flex-col gap-1.5 pt-2">
          <div className="flex items-center gap-1.5">
            <div className="relative flex-1">
              <Input
                className="pr-7"
                disabled={saving}
                onChange={(event) => setPassword(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    handleSave();
                  }
                }}
                placeholder={
                  hasPassword
                    ? m.upload_result_password_replace_placeholder()
                    : m.upload_settings_password_placeholder()
                }
                ref={inputRef}
                type={showPassword ? "text" : "password"}
                value={password}
              />
              <button
                aria-label={
                  showPassword
                    ? m.upload_result_password_hide()
                    : m.upload_result_password_show()
                }
                className="-translate-y-1/2 absolute top-1/2 right-1.5 text-current/60 hover:text-current"
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
            <Button
              className="shrink-0"
              disabled={password.trim().length === 0 || saving}
              onClick={handleSave}
              size="sm"
              type="button"
            >
              {m.upload_result_password_save()}
            </Button>
          </div>
          {hasPassword && (
            <Button
              className="w-fit px-0 text-destructive hover:bg-transparent hover:underline"
              disabled={saving}
              onClick={handleRemove}
              size="sm"
              type="button"
              variant="ghost"
            >
              {m.upload_result_password_remove()}
            </Button>
          )}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
