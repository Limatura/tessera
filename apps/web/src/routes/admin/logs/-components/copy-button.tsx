import { IconCheck, IconCopy } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import { useEffect, useRef, useState } from "react";

import { copyText } from "@/lib/copy-text";
import * as m from "@/paraglide/messages";

const COPIED_FEEDBACK_MS = 1500;

interface CopyButtonProps {
  /** What lands on the clipboard. A function, so a long line is only built when asked for. */
  text: () => string;
  /** Names what is copied, for the screen reader and the tooltip. */
  label: string;
  className?: string;
  /** Show the label next to the icon. */
  withLabel?: boolean;
}

/** Copies on press and says so for a moment. */
export function CopyButton({
  text,
  label,
  className,
  withLabel = false,
}: CopyButtonProps) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>();

  useEffect(() => () => clearTimeout(timer.current), []);

  const handleCopy = async () => {
    if (!(await copyText(text()))) {
      return;
    }
    setCopied(true);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setCopied(false), COPIED_FEEDBACK_MS);
  };

  return (
    <Button
      aria-label={copied ? m.logs_copied() : label}
      className={className}
      onClick={handleCopy}
      size={withLabel ? "sm" : "icon-sm"}
      title={label}
      type="button"
      variant="ghost"
    >
      {copied ? <IconCheck /> : <IconCopy />}
      {withLabel && (copied ? m.logs_copied() : label)}
    </Button>
  );
}
