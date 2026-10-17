import { IconDeviceMobileDown, IconShare2 } from "@tabler/icons-react";
import { Button, buttonVariants } from "@tessera/ui/components/button";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
import { useState, useSyncExternalStore } from "react";

import {
  getInstallMode,
  promptInstall,
  subscribeToInstallMode,
} from "@/lib/pwa/install-prompt";
import * as m from "@/paraglide/messages";

const serverMode = () => null;

/**
 * Installs the site as an app, where the browser allows it. Hidden when it
 * does not, or when this already is the installed app.
 */
export function InstallAppButton() {
  const mode = useSyncExternalStore(
    subscribeToInstallMode,
    getInstallMode,
    serverMode
  );
  const [showIosSteps, setShowIosSteps] = useState(false);

  if (!mode) {
    return null;
  }

  const handleClick = () => {
    if (mode === "ios") {
      setShowIosSteps(true);
      return;
    }
    promptInstall();
  };

  return (
    <>
      <button
        aria-label={m.install_app()}
        className={buttonVariants({ size: "icon", variant: "outline" })}
        onClick={handleClick}
        title={m.install_app()}
        type="button"
      >
        <IconDeviceMobileDown className="size-4" />
      </button>
      <ResponsiveDialog onOpenChange={setShowIosSteps} open={showIosSteps}>
        <ResponsiveDialogContent>
          <ResponsiveDialogHeader>
            <ResponsiveDialogTitle>{m.install_app()}</ResponsiveDialogTitle>
            <ResponsiveDialogDescription>
              {m.install_app_ios_description()}
            </ResponsiveDialogDescription>
          </ResponsiveDialogHeader>
          <ol className="flex list-decimal flex-col gap-2 pl-5 text-sm">
            <li>
              {m.install_app_ios_step_share()}{" "}
              <IconShare2
                aria-hidden
                className="inline size-4 align-text-bottom"
              />
            </li>
            <li>{m.install_app_ios_step_add()}</li>
          </ol>
          <ResponsiveDialogFooter>
            <Button onClick={() => setShowIosSteps(false)} type="button">
              {m.install_app_ios_done()}
            </Button>
          </ResponsiveDialogFooter>
        </ResponsiveDialogContent>
      </ResponsiveDialog>
    </>
  );
}
