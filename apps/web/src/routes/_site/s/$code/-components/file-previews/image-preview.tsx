import type { FilePreviewProps } from "@/routes/_site/s/$code/-components/file-previews/types";

export function ImagePreview({ filename, url }: FilePreviewProps) {
  return (
    <img
      alt={filename}
      className="max-h-96 w-full rounded-md border border-border/60 bg-muted/20 object-contain"
      src={url}
    />
  );
}
