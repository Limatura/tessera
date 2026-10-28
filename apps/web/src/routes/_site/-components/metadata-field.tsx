import { Label } from "@tessera/ui/components/label";
import { Switch } from "@tessera/ui/components/switch";
import { MapPinOff } from "lucide-react";

import * as m from "@/paraglide/messages";

/** Only files carry a photo's metadata, and only where the instance asks. */
export function offersMetadata(
  capabilities: { strip_metadata: boolean } | undefined,
  tab: string
) {
  return capabilities?.strip_metadata === true && tab === "file";
}

/** Whether an upload removes it: offered, and the uploader did not opt out. */
export function stripsMetadata(offered: boolean, keep: boolean) {
  return offered && !keep;
}

/**
 * Whether to send photos as they are. Off by default: with the instance's
 * `strip_metadata` on, JPEG, PNG and WebP files lose their location, camera
 * and time before they are sealed and sent. Decided before the upload, since
 * the bytes are changed on the way out.
 */
export function MetadataField({
  keep,
  onChange,
}: {
  keep: boolean;
  onChange: (keep: boolean) => void;
}) {
  return (
    <div className="flex flex-col gap-3 border-border/60 border-t pt-3">
      <div className="flex items-center justify-between gap-2">
        <Label htmlFor="upload-keep-metadata">
          <MapPinOff className="size-3.5 text-sky-500" />
          {m.upload_keep_metadata()}
        </Label>
        <Switch
          checked={keep}
          id="upload-keep-metadata"
          onCheckedChange={onChange}
        />
      </div>
      {keep || (
        <p className="text-muted-foreground text-xs">
          {m.upload_keep_metadata_hint()}
        </p>
      )}
    </div>
  );
}
