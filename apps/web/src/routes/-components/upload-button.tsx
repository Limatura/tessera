import { cn } from "@tessera/ui/lib/utils";
import { Check, Upload } from "lucide-react";
import { motion } from "motion/react";
import { forwardRef } from "react";

const RADIUS = 54;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

interface UploadButtonProps {
  status: "idle" | "uploading" | "done";
  progress: number;
  onClick: () => void;
}

export const UploadButton = forwardRef<HTMLButtonElement, UploadButtonProps>(
  ({ status, progress, onClick }, ref) => {
    const dashOffset = CIRCUMFERENCE * (1 - progress / 100);

    return (
      <button
        className={cn(
          "relative flex h-36 w-36 items-center justify-center rounded-full border border-primary/40 bg-primary text-primary-foreground shadow-primary/30 shadow-[0_0_40px_-8px] transition-all",
          status === "idle" && "hover:scale-[1.02] hover:shadow-primary/40"
        )}
        disabled={status !== "idle"}
        onClick={onClick}
        ref={ref}
        type="button"
      >
        <svg
          className="-rotate-90 absolute inset-0 h-full w-full"
          viewBox="0 0 120 120"
        >
          <title>upload progress</title>
          <circle
            className="text-primary-foreground/25"
            cx="60"
            cy="60"
            fill="none"
            r={RADIUS}
            stroke="currentColor"
            strokeWidth="2"
          />
          {status === "uploading" && (
            <motion.circle
              animate={{ strokeDashoffset: dashOffset }}
              className="text-primary-foreground"
              cx="60"
              cy="60"
              fill="none"
              r={RADIUS}
              stroke="currentColor"
              strokeDasharray={CIRCUMFERENCE}
              strokeLinecap="round"
              strokeWidth="2"
              transition={{ duration: 0.2, ease: "linear" }}
            />
          )}
        </svg>

        <motion.div
          animate={{ scale: 1 }}
          className="flex flex-col items-center gap-1.5"
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
    );
  }
);
UploadButton.displayName = "UploadButton";
