import { Button } from "@tessera/ui/components/button";
import { formatBytes } from "@tessera/ui/lib/format";
import { File as FileIcon } from "lucide-react";
import { useRef } from "react";

import * as m from "@/paraglide/messages";

import { QuickSubmitButton } from "./quick-submit-button";
import type { SubmitStatus } from "./use-submit-transition";

/** Names past this many collapse into "and N more". */
const LISTED = 4;

interface SharedFilesProps {
  files: File[];
  status: SubmitStatus;
  progress: number;
  onUpload: (el: HTMLButtonElement | null) => void;
  onDiscard: () => void;
}

/**
 * Files handed over by the system share sheet, waiting for a click.
 *
 * Picking files in the upload button sends them at once, since the picker was
 * the decision. A share from another app is not: it may be the wrong file, and
 * the expiry, password and encryption below should be set first.
 */
export function SharedFiles({
  files,
  status,
  progress,
  onUpload,
  onDiscard,
}: SharedFilesProps) {
  const buttonRef = useRef<HTMLButtonElement>(null);
  const total = files.reduce((sum, file) => sum + file.size, 0);
  const hidden = files.length - LISTED;

  return (
    <div className="flex flex-col gap-3 py-2">
      <div className="flex items-baseline justify-between gap-2 text-sm">
        <span className="font-medium">
          {m.upload_shared_files_title({ count: files.length })}
        </span>
        <span className="text-muted-foreground tabular-nums">
          {formatBytes(total)}
        </span>
      </div>
      <ul className="flex flex-col gap-1.5 text-sm">
        {files.slice(0, LISTED).map((file, index) => (
          <li
            className="flex min-w-0 items-center gap-2"
            // Two shared files can have the same name
            // oxlint-disable-next-line react/no-array-index-key
            key={`${index}-${file.name}`}
          >
            <FileIcon className="size-4 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate">{file.name}</span>
            <span className="shrink-0 text-muted-foreground text-xs tabular-nums">
              {formatBytes(file.size)}
            </span>
          </li>
        ))}
        {hidden > 0 && (
          <li className="text-muted-foreground text-xs">
            {m.upload_shared_files_more({ count: hidden })}
          </li>
        )}
      </ul>
      <div className="flex items-center justify-end gap-2">
        <Button
          disabled={status !== "idle"}
          onClick={onDiscard}
          size="sm"
          variant="ghost"
        >
          {m.upload_shared_files_discard()}
        </Button>
        <QuickSubmitButton
          label={m.upload_shared_files_upload()}
          onClick={() => onUpload(buttonRef.current)}
          progress={progress}
          ref={buttonRef}
          status={status}
        />
      </div>
    </div>
  );
}
