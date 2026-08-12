import { Input } from "@tessera/ui/components/input";
import { useRef } from "react";

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
      <Input placeholder="https://example.com/very/long/url" type="url" />
      <QuickSubmitButton
        label="Shorten link"
        onClick={() => onSubmit(buttonRef.current)}
        progress={progress}
        ref={buttonRef}
        status={status}
      />
    </div>
  );
}
