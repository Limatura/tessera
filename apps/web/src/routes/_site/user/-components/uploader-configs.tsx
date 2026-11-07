import { useCapabilities } from "@/hooks/use-capabilities";
import { uploaderOrigin } from "@/lib/uploaders/origin";
import * as m from "@/paraglide/messages";

import { ShellScriptDownload } from "./shell-script-download";
import { ShareXDownload } from "./sharex-download";

/** What every uploader config is written from. */
export interface UploaderContext {
  /** Where uploads go, without a trailing slash. */
  origin: string;
  /** The key just minted. Only on screen once, so configs are made now. */
  secret: string;
}

/**
 * One tool's config. Each tool (ShareX, the shell script, the iOS Shortcut)
 * adds its own entry to `UPLOADERS`.
 */
interface Uploader {
  id: string;
  render: (context: UploaderContext) => React.ReactNode;
}

const UPLOADERS: readonly Uploader[] = [
  { id: "shell", render: (context) => <ShellScriptDownload {...context} /> },
  { id: "sharex", render: (context) => <ShareXDownload {...context} /> },
];

/**
 * The configs that can be made from a freshly minted key. Shown only when the
 * key may upload: a read-only key in a screenshot tool would fail on its first
 * use.
 */
export function UploaderConfigs({
  permissions,
  secret,
}: {
  permissions: readonly string[];
  secret: string;
}) {
  const { data: capabilities } = useCapabilities();

  if (UPLOADERS.length === 0 || !permissions.includes("shares:create")) {
    return null;
  }
  const context: UploaderContext = {
    origin: uploaderOrigin(capabilities?.public_url, window.location.origin),
    secret,
  };
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap gap-2">
        {UPLOADERS.map((uploader) => (
          <div key={uploader.id}>{uploader.render(context)}</div>
        ))}
      </div>
      <p className="text-muted-foreground text-xs">
        {m.user_api_keys_uploader_warning()}
      </p>
    </div>
  );
}
