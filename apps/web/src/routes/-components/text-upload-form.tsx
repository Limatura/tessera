import { Textarea } from "@tessera/ui/components/textarea";
import { useRef } from "react";

import type { SubmitStatus } from "../../hooks/use-submit-transition";
import { QuickSubmitButton } from "./quick-submit-button";

interface TextUploadFormProps {
  status: SubmitStatus;
  progress: number;
  onSubmit: (el: HTMLButtonElement | null) => void;
}

export function TextUploadForm({
  status,
  progress,
  onSubmit,
}: TextUploadFormProps) {
  const buttonRef = useRef<HTMLButtonElement>(null);

  return (
    <div className="flex flex-col gap-3">
      <Textarea placeholder="Paste or type text to share..." rows={6} />
      <div className="flex justify-end">
        <QuickSubmitButton
          label="Send text"
          onClick={() => onSubmit(buttonRef.current)}
          progress={progress}
          ref={buttonRef}
          status={status}
        />
      </div>
    </div>
  );
}
