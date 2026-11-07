import { IconTerminal2 } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";

import { downloadText } from "@/lib/uploaders/download";
import { SHELL_SCRIPT_FILE_NAME, shellScript } from "@/lib/uploaders/shell";
import * as m from "@/paraglide/messages";

import type { UploaderContext } from "./uploader-configs";

/** Downloads `tessera-upload` with this key already filled in. */
export function ShellScriptDownload({ origin, secret }: UploaderContext) {
  const handleDownload = () =>
    downloadText(
      SHELL_SCRIPT_FILE_NAME,
      shellScript({ origin, secret }),
      "text/x-shellscript"
    );

  return (
    <Button onClick={handleDownload} type="button" variant="outline">
      <IconTerminal2 size={16} />
      {m.user_api_keys_uploader_shell()}
    </Button>
  );
}
