import type { components } from "@tessera/api/schema";
import { cn } from "@tessera/ui/lib/utils";
import {
  ShieldAlert,
  ShieldCheck,
  ShieldEllipsis,
  ShieldQuestion,
} from "lucide-react";

import * as m from "@/paraglide/messages";

type ShareResponse = components["schemas"]["ShareResponse"];

/**
 * What the malware scanner made of a share, for its uploader. Nothing when
 * scanning is off: there is nothing to say.
 *
 * Colours are explicit, not theme tokens, because it sits on the upload
 * overlay as well as on cards, and must read as a warning on both.
 */
export function ScanStatus({
  share,
  className,
}: {
  share: Pick<ShareResponse, "scan_state" | "scan_detail">;
  className?: string;
}) {
  const view = scanView(share);
  if (!view) {
    return null;
  }
  const { icon: Icon, tone, title, description } = view;
  return (
    <output
      aria-live="polite"
      className={cn(
        "flex items-start gap-2 rounded-lg border px-3 py-2 text-left text-sm",
        TONE[tone],
        className
      )}
    >
      <Icon
        aria-hidden
        className={cn(
          "mt-0.5 size-4 shrink-0",
          share.scan_state === "pending" && "animate-pulse"
        )}
      />
      <div className="flex flex-col gap-0.5">
        <span className="font-medium">{title}</span>
        {description && (
          <span className="text-xs opacity-80">{description}</span>
        )}
      </div>
    </output>
  );
}

type Tone = "pending" | "clean" | "danger" | "info";

const TONE: Record<Tone, string> = {
  clean:
    "border-green-500/40 bg-green-500/10 text-green-700 dark:text-green-400",
  danger: "border-red-500/50 bg-red-500/10 text-red-700 dark:text-red-400",
  info: "border-current/15 bg-current/5",
  pending:
    "border-amber-500/50 bg-amber-500/10 text-amber-700 dark:text-amber-400",
};

function scanView(share: Pick<ShareResponse, "scan_state" | "scan_detail">) {
  switch (share.scan_state) {
    case "pending": {
      return {
        description: m.scan_status_pending_description(),
        icon: ShieldEllipsis,
        title: m.scan_status_pending(),
        tone: "pending" as const,
      };
    }
    case "clean": {
      return {
        description: undefined,
        icon: ShieldCheck,
        title: m.scan_status_clean(),
        tone: "clean" as const,
      };
    }
    case "infected": {
      return {
        description: m.scan_status_infected_description({
          threat: share.scan_detail ?? "?",
        }),
        icon: ShieldAlert,
        title: m.scan_status_infected(),
        tone: "danger" as const,
      };
    }
    case "skipped": {
      return {
        description: undefined,
        icon: ShieldQuestion,
        title:
          share.scan_detail === "encrypted"
            ? m.scan_status_skipped_encrypted()
            : m.scan_status_skipped_too_large(),
        tone: "info" as const,
      };
    }
    case "deferred": {
      return {
        description: m.scan_status_deferred_description(),
        icon: ShieldQuestion,
        title: m.scan_status_deferred(),
        tone: "info" as const,
      };
    }
    default: {
      return null;
    }
  }
}
