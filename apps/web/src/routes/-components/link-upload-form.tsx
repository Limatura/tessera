import { Input } from "@tessera/ui/components/input";
import { useRef, useState } from "react";

import * as m from "@/paraglide/messages";

import { QuickSubmitButton } from "./quick-submit-button";
import type { SubmitStatus } from "./use-submit-transition";

interface LinkUploadFormProps {
  status: SubmitStatus;
  progress: number;
  onSubmit: (el: HTMLButtonElement | null, payload: { url: string }) => void;
}

export function LinkUploadForm({
  status,
  progress,
  onSubmit,
}: LinkUploadFormProps) {
  const buttonRef = useRef<HTMLButtonElement>(null);
  const [url, setUrl] = useState("");

  const handleSubmit = () => {
    if (url.trim().length > 0) {
      onSubmit(buttonRef.current, { url: url.trim() });
    }
  };

  return (
    <div className="flex items-center gap-3">
      <Input
        onChange={(event) => setUrl(event.target.value)}
        placeholder={m.upload_link_placeholder()}
        type="url"
        value={url}
      />
      <QuickSubmitButton
        label={m.upload_shorten_link()}
        onClick={handleSubmit}
        progress={progress}
        ref={buttonRef}
        status={status}
      />
    </div>
  );
}
