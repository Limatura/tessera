import { IconClock } from "@tabler/icons-react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@tessera/ui/components/alert-dialog";
import { Button } from "@tessera/ui/components/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@tessera/ui/components/collapsible";
import { Input } from "@tessera/ui/components/input";
import { Label } from "@tessera/ui/components/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { Switch } from "@tessera/ui/components/switch";
import { toast } from "@tessera/ui/components/toast";
import {
  Check,
  ChevronDown,
  Copy,
  Eye,
  EyeOff,
  Flame,
  Lock,
  RotateCcw,
  Trash2,
} from "lucide-react";
import { motion } from "motion/react";
import { QRCodeSVG } from "qrcode.react";
import { useRef, useState } from "react";

import { deleteShare, updateShare } from "@/lib/upload";
import type { ShareSettings, UploadResultData } from "@/lib/upload";
import * as m from "@/paraglide/messages";

const EXPIRY_OPTIONS = [
  { hours: 1, label: () => m.upload_settings_expiry_1h() },
  { hours: 24, label: () => m.upload_settings_expiry_24h() },
  { hours: 24 * 7, label: () => m.upload_settings_expiry_7d() },
  { hours: 0, label: () => m.upload_settings_expiry_never() },
] as const;

interface UploadResultProps {
  onReset: () => void;
  result: UploadResultData;
}

export function UploadResult({ onReset, result }: UploadResultProps) {
  const [copied, setCopied] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [expiresInHours, setExpiresInHours] = useState(result.expiresInHours);
  const [burnAfterReading, setBurnAfterReading] = useState(
    result.maxUses === 1
  );
  const [hasPassword, setHasPassword] = useState(result.hasPassword);
  const [passwordOpen, setPasswordOpen] = useState(false);
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [savingPassword, setSavingPassword] = useState(false);
  const passwordInputRef = useRef<HTMLInputElement>(null);

  const applySettings = async (
    settings: ShareSettings,
    rollback: () => void
  ) => {
    try {
      await updateShare(result.code, settings, result.managementToken);
    } catch (error) {
      rollback();
      toast.add({
        description:
          error instanceof Error ? error.message : m.common_try_again(),
        title: m.upload_result_update_failed(),
        type: "error",
      });
    }
  };

  const handleExpiryChange = (value: string | null) => {
    const previous = expiresInHours;
    const hours = Number(value ?? 24);
    const next = hours === 0 ? undefined : hours;
    setExpiresInHours(next);
    applySettings({ expiresInHours: next }, () => setExpiresInHours(previous));
  };

  const handleBurnAfterReadingChange = (checked: boolean) => {
    setBurnAfterReading(checked);
    applySettings({ maxUses: checked ? 1 : undefined }, () =>
      setBurnAfterReading(!checked)
    );
  };

  const handlePasswordOpenChange = (open: boolean) => {
    setPasswordOpen(open);
    setPassword("");
    setShowPassword(false);
    if (open) {
      requestAnimationFrame(() => passwordInputRef.current?.focus());
    }
  };

  const handlePasswordSave = async () => {
    const trimmed = password.trim();
    setSavingPassword(true);
    try {
      await updateShare(
        result.code,
        { password: trimmed.length > 0 ? trimmed : undefined },
        result.managementToken
      );
      setHasPassword(trimmed.length > 0);
      setPasswordOpen(false);
      setPassword("");
      setShowPassword(false);
    } catch (error) {
      toast.add({
        description:
          error instanceof Error ? error.message : m.common_try_again(),
        title: m.upload_result_update_failed(),
        type: "error",
      });
    } finally {
      setSavingPassword(false);
    }
  };

  const handlePasswordRemove = async () => {
    setSavingPassword(true);
    try {
      await updateShare(
        result.code,
        { password: undefined },
        result.managementToken
      );
      setHasPassword(false);
      setPasswordOpen(false);
      setPassword("");
    } catch (error) {
      toast.add({
        description:
          error instanceof Error ? error.message : m.common_try_again(),
        title: m.upload_result_update_failed(),
        type: "error",
      });
    } finally {
      setSavingPassword(false);
    }
  };

  const handleCopy = async () => {
    await navigator.clipboard.writeText(result.shortUrl);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  const handleDelete = async () => {
    setDeleting(true);
    try {
      await deleteShare(result.code, result.managementToken);
      onReset();
    } catch (error) {
      toast.add({
        description:
          error instanceof Error ? error.message : m.common_try_again(),
        title: m.upload_result_delete_failed(),
        type: "error",
      });
    } finally {
      setDeleting(false);
    }
  };

  return (
    <motion.div
      animate={{ opacity: 1, y: 0 }}
      className="flex w-full max-w-sm flex-col items-center gap-6 text-center"
      initial={{ opacity: 0, y: 12 }}
      transition={{ delay: 0.15, duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
    >
      <div className="rounded-xl border border-neutral-900/15 bg-neutral-900/5 p-3">
        <QRCodeSVG size={160} value={result.shortUrl} />
      </div>

      <div className="flex flex-col gap-1">
        <p className="font-medium text-neutral-900 text-sm">
          {result.fileName}
        </p>
        <p className="text-neutral-900/70 text-xs">{m.upload_result_ready()}</p>
      </div>

      <div className="flex w-full items-center gap-2 rounded-lg border border-neutral-900/15 bg-neutral-900/5 px-3 py-2">
        <span className="flex-1 truncate text-left font-mono text-neutral-900 text-xs">
          {result.shortUrl}
        </span>
        <Button
          className="shrink-0 text-neutral-900 hover:bg-neutral-900/10 hover:text-neutral-900"
          onClick={handleCopy}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          {copied ? <Check /> : <Copy />}
        </Button>
      </div>

      <div className="flex w-full flex-col gap-3 rounded-lg border border-neutral-900/15 bg-neutral-900/5 p-3 text-left">
        <div className="flex items-center justify-between gap-2">
          <Label className="text-neutral-900" htmlFor="share-result-expiry">
            <IconClock className="size-3.5" />
            {m.upload_settings_expiry_label()}
          </Label>
          <Select
            onValueChange={handleExpiryChange}
            value={String(expiresInHours ?? 0)}
          >
            <SelectTrigger id="share-result-expiry" size="sm">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {EXPIRY_OPTIONS.map((option) => (
                <SelectItem key={option.hours} value={String(option.hours)}>
                  {option.label()}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="flex items-center justify-between gap-2">
          <Label
            className="text-neutral-900"
            htmlFor="share-result-burn-after-reading"
          >
            <Flame className="size-3.5" />
            {m.upload_settings_burn_after_reading()}
          </Label>
          <Switch
            checked={burnAfterReading}
            id="share-result-burn-after-reading"
            onCheckedChange={handleBurnAfterReadingChange}
          />
        </div>

        <Collapsible
          onOpenChange={handlePasswordOpenChange}
          open={passwordOpen}
        >
          <CollapsibleTrigger
            className="group flex w-full items-center justify-between gap-2 text-neutral-900"
            render={
              <button
                aria-label={
                  hasPassword
                    ? m.upload_result_password_set()
                    : m.upload_result_password_unset()
                }
                type="button"
              />
            }
          >
            <span className="flex items-center gap-2 text-xs/relaxed font-medium">
              <Lock className="size-3.5" />
              {hasPassword
                ? m.upload_result_password_set()
                : m.upload_result_password_unset()}
            </span>
            <ChevronDown className="size-3.5 text-neutral-900/60 transition-transform group-data-panel-open:rotate-180" />
          </CollapsibleTrigger>

          <CollapsibleContent>
            <div className="flex flex-col gap-1.5 pt-2">
              <div className="flex items-center gap-1.5">
                <div className="relative flex-1">
                  <Input
                    className="pr-7"
                    disabled={savingPassword}
                    onChange={(event) => setPassword(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        handlePasswordSave();
                      }
                    }}
                    placeholder={
                      hasPassword
                        ? m.upload_result_password_replace_placeholder()
                        : m.upload_settings_password_placeholder()
                    }
                    ref={passwordInputRef}
                    type={showPassword ? "text" : "password"}
                    value={password}
                  />
                  <button
                    aria-label={
                      showPassword
                        ? m.upload_result_password_hide()
                        : m.upload_result_password_show()
                    }
                    className="-translate-y-1/2 absolute top-1/2 right-1.5 text-neutral-900/60 hover:text-neutral-900"
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
                  disabled={password.trim().length === 0 || savingPassword}
                  onClick={handlePasswordSave}
                  size="sm"
                  type="button"
                >
                  {m.upload_result_password_save()}
                </Button>
              </div>
              {hasPassword && (
                <Button
                  className="w-fit px-0 text-destructive hover:bg-transparent hover:underline"
                  disabled={savingPassword}
                  onClick={handlePasswordRemove}
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
      </div>

      <div className="flex w-full items-center gap-2">
        <Button
          className="flex-1 border-neutral-900/25 text-neutral-900 hover:bg-neutral-900/10 hover:text-neutral-900"
          onClick={onReset}
          type="button"
          variant="outline"
        >
          <RotateCcw />
          {m.upload_result_upload_another()}
        </Button>

        <AlertDialog>
          <AlertDialogTrigger
            render={
              <Button
                aria-label={m.upload_result_delete()}
                className="shrink-0 border-neutral-900/25 text-neutral-900 hover:bg-destructive/20 hover:text-destructive"
                size="icon"
                type="button"
                variant="outline"
              />
            }
          >
            <Trash2 />
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>
                {m.upload_result_delete_confirm_title()}
              </AlertDialogTitle>
              <AlertDialogDescription>
                {m.upload_result_delete_confirm_description()}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>{m.upload_result_cancel()}</AlertDialogCancel>
              <AlertDialogAction
                disabled={deleting}
                onClick={handleDelete}
                variant="destructive"
              >
                {m.upload_result_delete()}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </motion.div>
  );
}
