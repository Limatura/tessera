import { cn } from "@tessera/ui/lib/utils";
import { Check, Send } from "lucide-react";
import { forwardRef } from "react";

import { BlobCanvas } from "./blob-canvas";
import { PRIMARY_CLUSTER_SPECS, PRIMARY_MELT_COLOR } from "./blob-specs";

interface QuickSubmitButtonProps {
  status: "idle" | "uploading" | "done";
  progress: number;
  onClick: () => void;
  label?: string;
}

export const QuickSubmitButton = forwardRef<
  HTMLButtonElement,
  QuickSubmitButtonProps
>(({ status, progress, onClick, label = "Send" }, ref) => (
  <button
    aria-label={label}
    className={cn(
      "relative flex h-14 w-14 shrink-0 items-center justify-center text-primary-foreground transition-transform",
      status === "idle" && "hover:scale-[1.05]"
    )}
    disabled={status !== "idle"}
    onClick={onClick}
    ref={ref}
    type="button"
  >
    <BlobCanvas
      className="absolute inset-0 h-full w-full"
      progress={status === "idle" ? 0 : progress}
      specs={PRIMARY_CLUSTER_SPECS}
      toColor={PRIMARY_MELT_COLOR}
    />
    <span className="relative">
      {status === "done" ? (
        <Check className="size-4" />
      ) : (
        <Send className="size-4" />
      )}
    </span>
  </button>
));
QuickSubmitButton.displayName = "QuickSubmitButton";
