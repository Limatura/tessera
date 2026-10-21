import { AudioPlayer } from "@/components/audio-player/audio-player";
import type { FilePreviewProps } from "@/routes/_site/s/$code/-components/file-previews/types";

export function AudioPreview({ filename, url }: FilePreviewProps) {
  return <AudioPlayer src={url} title={filename} />;
}
