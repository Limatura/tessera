import { cn } from "@tessera/ui/lib/utils";
import { Check, Upload } from "lucide-react";
import { motion } from "motion/react";
import { forwardRef } from "react";

import { BlobCanvas } from "./blob-canvas";
import { PRIMARY_CLUSTER_SPECS, PRIMARY_MELT_COLOR } from "./blob-specs";

interface UploadButtonProps {
  status: "idle" | "uploading" | "done";
  progress: number;
  onClick: () => void;
}

export const UploadButton = forwardRef<HTMLButtonElement, UploadButtonProps>(
  ({ status, progress, onClick }, ref) => (
    <button
      className={cn(
        "relative flex h-56 w-56 items-center justify-center text-primary-foreground transition-transform",
        status === "idle" && "hover:scale-[1.03]"
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

      <motion.div
        animate={{ scale: 1 }}
        className="relative flex flex-col items-center gap-1.5"
        initial={false}
        key={status}
      >
        {status === "done" ? (
          <Check className="size-6" />
        ) : (
          <Upload className="size-6" />
        )}
        <span className="text-primary-foreground/80 text-xs">
          {status === "idle" && "Drop or click to upload"}
          {status === "uploading" && `${Math.round(progress)}%`}
          {status === "done" && "Done"}
        </span>
      </motion.div>
    </button>
  )
);
UploadButton.displayName = "UploadButton";
