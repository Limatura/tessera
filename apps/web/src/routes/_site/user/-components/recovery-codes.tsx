import { IconCheck, IconCopy, IconDownload } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import { useState } from "react";

import { copyText } from "@/lib/copy-text";
import * as m from "@/paraglide/messages";

const COPIED_FEEDBACK_MS = 2000;

/**
 * Recovery codes as the server returned them. They are shown this once: the
 * server keeps only hashes, so copying or saving them is the user's only chance.
 */
export function RecoveryCodes({ codes }: { codes: string[] }) {
  const [copied, setCopied] = useState(false);
  const text = codes.join("\n");

  const handleCopy = async () => {
    if (!(await copyText(text))) {
      return;
    }
    setCopied(true);
    setTimeout(() => setCopied(false), COPIED_FEEDBACK_MS);
  };

  const handleDownload = () => {
    const url = URL.createObjectURL(
      new Blob([`${text}\n`], { type: "text/plain" })
    );
    const link = document.createElement("a");
    link.href = url;
    link.download = "tessera-recovery-codes.txt";
    link.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="space-y-3">
      <p className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm">
        {m.user_mfa_recovery_warning()}
      </p>
      <ul className="grid grid-cols-2 gap-2 rounded-lg border bg-muted/40 p-3 font-mono text-sm">
        {codes.map((code) => (
          <li key={code}>{code}</li>
        ))}
      </ul>
      <div className="flex gap-2">
        <Button onClick={handleCopy} size="sm" type="button" variant="outline">
          {copied ? <IconCheck size={16} /> : <IconCopy size={16} />}
          {copied ? m.user_mfa_recovery_copied() : m.user_mfa_recovery_copy()}
        </Button>
        <Button
          onClick={handleDownload}
          size="sm"
          type="button"
          variant="outline"
        >
          <IconDownload size={16} />
          {m.user_mfa_recovery_download()}
        </Button>
      </div>
    </div>
  );
}
