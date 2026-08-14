import { Input } from "@tessera/ui/components/input";
import { useRef } from "react";

import * as m from "@/paraglide/messages";

import { QuickSubmitButton } from "./quick-submit-button";
import type { SubmitStatus } from "./use-submit-transition";

interface LinkUploadFormProps {
  status: SubmitStatus;
  progress: number;
  onSubmit: (el: HTMLButtonElement | null) => void;
}

export function LinkUploadForm({
  status,
  progress,
  onSubmit,
}: LinkUploadFormProps) {
  const buttonRef = useRef<HTMLButtonElement>(null);

  return (
    <div className="flex items-center gap-3">
      <Input placeholder={m.upload_link_placeholder()} type="url" />
      <QuickSubmitButton
        label={m.upload_shorten_link()}
        onClick={() => onSubmit(buttonRef.current)}
        progress={progress}
        ref={buttonRef}
        status={status}
      />
    </div>
  );
}
