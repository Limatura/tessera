import { Label } from "@tessera/ui/components/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { Textarea } from "@tessera/ui/components/textarea";
import { useRef } from "react";

import type { SubmitStatus } from "../../hooks/use-submit-transition";
import { QuickSubmitButton } from "./quick-submit-button";

const LANGUAGES = [
  { label: "Plain text", value: "plaintext" },
  { label: "JavaScript", value: "javascript" },
  { label: "TypeScript", value: "typescript" },
  { label: "Python", value: "python" },
  { label: "Rust", value: "rust" },
  { label: "Go", value: "go" },
];

interface CodeUploadFormProps {
  status: SubmitStatus;
  progress: number;
  onSubmit: (el: HTMLButtonElement | null) => void;
}

export function CodeUploadForm({
  status,
  progress,
  onSubmit,
}: CodeUploadFormProps) {
  const buttonRef = useRef<HTMLButtonElement>(null);

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <Label htmlFor="code-language">Language</Label>
        <Select defaultValue="plaintext">
          <SelectTrigger id="code-language" size="sm">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {LANGUAGES.map((lang) => (
              <SelectItem key={lang.value} value={lang.value}>
                {lang.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <Textarea
        className="font-mono"
        placeholder="paste your snippet..."
        rows={8}
      />
      <div className="flex justify-end">
        <QuickSubmitButton
          label="Send snippet"
          onClick={() => onSubmit(buttonRef.current)}
          progress={progress}
          ref={buttonRef}
          status={status}
        />
      </div>
    </div>
  );
}
