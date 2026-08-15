import { cn } from "@tessera/ui/lib/utils";
import { Check, Upload } from "lucide-react";
import { motion } from "motion/react";
import { forwardRef, useRef, useState } from "react";

import * as m from "@/paraglide/messages";

import { BlobCanvas } from "./blob-canvas";
import { PRIMARY_CLUSTER_SPECS, PRIMARY_MELT_COLOR } from "./blob-specs";

interface UploadButtonProps {
  status: "idle" | "uploading" | "done";
  progress: number;
  onSelectFile: (file: File) => void;
}

export const UploadButton = forwardRef<HTMLButtonElement, UploadButtonProps>(
  ({ status, progress, onSelectFile }, ref) => {
    const inputRef = useRef<HTMLInputElement>(null);
    const [isDragging, setIsDragging] = useState(false);

    const handleClick = () => {
      if (status !== "idle") {
        return;
      }
      inputRef.current?.click();
    };

    const handleInputChange = (event: React.ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      event.target.value = "";
      if (file) {
        onSelectFile(file);
      }
    };

    const handleDragOver = (event: React.DragEvent<HTMLButtonElement>) => {
      event.preventDefault();
      if (status === "idle") {
        setIsDragging(true);
      }
    };

    const handleDragLeave = () => {
      setIsDragging(false);
    };

    const handleDrop = (event: React.DragEvent<HTMLButtonElement>) => {
      event.preventDefault();
      setIsDragging(false);
      if (status !== "idle") {
        return;
      }
      const file = event.dataTransfer.files?.[0];
      if (file) {
        onSelectFile(file);
      }
    };

    return (
      <button
        className={cn(
          "relative flex h-56 w-56 items-center justify-center text-primary-foreground transition-transform",
          status === "idle" && "hover:scale-[1.03]",
          isDragging && "scale-[1.05]"
        )}
        disabled={status !== "idle"}
        onClick={handleClick}
        onDragLeave={handleDragLeave}
        onDragOver={handleDragOver}
        onDrop={handleDrop}
        ref={ref}
        type="button"
      >
        <input
          accept="*/*"
          className="hidden"
          onChange={handleInputChange}
          ref={inputRef}
          type="file"
        />

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
            {status === "idle" && m.upload_button_drop_or_click()}
            {status === "uploading" &&
              m.upload_button_uploading({ progress: Math.round(progress) })}
            {status === "done" && m.upload_button_done()}
          </span>
        </motion.div>
      </button>
    );
  }
);
UploadButton.displayName = "UploadButton";
