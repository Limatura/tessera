import { Button } from "@tessera/ui/components/button";
import { Check, Copy, RotateCcw } from "lucide-react";
import { motion } from "motion/react";
import { QRCodeSVG } from "qrcode.react";
import { useState } from "react";

const PLACEHOLDER_RESULT = {
  fileName: "design-notes.pdf",
  shortUrl: "https://tsr.sh/x7Qk2p",
};

interface UploadResultProps {
  onReset: () => void;
}

export function UploadResult({ onReset }: UploadResultProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(PLACEHOLDER_RESULT.shortUrl);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <motion.div
      animate={{ opacity: 1, y: 0 }}
      className="flex w-full max-w-sm flex-col items-center gap-6 text-center"
      initial={{ opacity: 0, y: 12 }}
      transition={{ delay: 0.15, duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
    >
      <div className="rounded-xl border border-neutral-900/15 bg-neutral-900/5 p-3">
        <QRCodeSVG size={160} value={PLACEHOLDER_RESULT.shortUrl} />
      </div>

      <div className="flex flex-col gap-1">
        <p className="font-medium text-neutral-900 text-sm">
          {PLACEHOLDER_RESULT.fileName}
        </p>
        <p className="text-neutral-900/70 text-xs">Ready to share</p>
      </div>

      <div className="flex w-full items-center gap-2 rounded-lg border border-neutral-900/15 bg-neutral-900/5 px-3 py-2">
        <span className="flex-1 truncate text-left font-mono text-neutral-900 text-xs">
          {PLACEHOLDER_RESULT.shortUrl}
        </span>
        <Button
          className="shrink-0 text-neutral-900 hover:bg-neutral-900/10 hover:text-neutral-900"
          onClick={handleCopy}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          {copied ? <Check /> : <Copy />}
        </Button>
      </div>

      <Button
        className="border-neutral-900/25 text-neutral-900 hover:bg-neutral-900/10 hover:text-neutral-900"
        onClick={onReset}
        type="button"
        variant="outline"
      >
        <RotateCcw />
        Upload another
      </Button>
    </motion.div>
  );
}
