import { IconDownload } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";

import { downloadText } from "@/lib/uploaders/download";
import { shareXConfig, shareXFileName } from "@/lib/uploaders/sharex";
import * as m from "@/paraglide/messages";

import type { UploaderContext } from "./uploader-configs";

/** Downloads a ShareX custom uploader with this key already filled in. */
export function ShareXDownload({ origin, secret }: UploaderContext) {
  const handleDownload = () => {
    const config = shareXConfig({
      name: `Tessera (${new URL(origin).host})`,
      origin,
      secret,
    });
    downloadText(
      shareXFileName(origin),
      `${JSON.stringify(config, null, 2)}\n`,
      "application/json"
    );
  };

  return (
    <Button onClick={handleDownload} type="button" variant="outline">
      <IconDownload size={16} />
      {m.user_api_keys_uploader_sharex()}
    </Button>
  );
}
