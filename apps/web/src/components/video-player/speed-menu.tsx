import { Menu } from "@base-ui/react/menu";
import { Check } from "lucide-react";

import * as m from "@/paraglide/messages";

const RATES = [0.5, 0.75, 1, 1.25, 1.5, 2] as const;

interface SpeedMenuProps {
  rate: number;
  onRateChange: (rate: number) => void;
  onOpenChange: (open: boolean) => void;
  /**
   * Where the popup is mounted. It has to be inside the player: in fullscreen
   * only the fullscreen element's subtree is drawn, so a popup in `<body>`
   * would open where nobody can see it.
   */
  container: React.RefObject<HTMLElement | null>;
}

/**
 * Built on Base UI's menu directly rather than `DropdownMenu`, which always
 * portals into `<body>` and is styled for the page, not for a dark picture.
 */
export function SpeedMenu({
  container,
  onOpenChange,
  onRateChange,
  rate,
}: SpeedMenuProps) {
  return (
    <Menu.Root onOpenChange={(open) => onOpenChange(open)}>
      <Menu.Trigger
        aria-label={m.video_speed()}
        className="flex h-8 min-w-10 items-center justify-center rounded-md px-1.5 font-medium text-white/90 text-xs tabular-nums outline-none transition-colors hover:bg-white/10 hover:text-white focus-visible:ring-2 focus-visible:ring-white/70 data-popup-open:bg-white/15"
      >
        {formatRate(rate)}
      </Menu.Trigger>
      <Menu.Portal container={container}>
        <Menu.Positioner
          align="end"
          className="z-20 outline-none"
          side="top"
          sideOffset={8}
        >
          <Menu.Popup className="min-w-36 origin-(--transform-origin) rounded-md bg-black/85 p-1 text-white text-xs ring-1 ring-white/10 backdrop-blur-md outline-none duration-100 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95">
            <Menu.RadioGroup
              onValueChange={(value: number) => onRateChange(value)}
              value={rate}
            >
              <Menu.GroupLabel className="px-2 pt-1 pb-1.5 text-white/60">
                {m.video_speed()}
              </Menu.GroupLabel>
              {RATES.map((option) => (
                <Menu.RadioItem
                  className="flex cursor-pointer items-center gap-2 rounded-sm py-1.5 pr-2 pl-7 tabular-nums outline-none data-highlighted:bg-white/15 relative"
                  closeOnClick
                  key={option}
                  value={option}
                >
                  <Menu.RadioItemIndicator className="absolute left-2 flex">
                    <Check className="size-3.5" />
                  </Menu.RadioItemIndicator>
                  {option === 1 ? m.video_speed_normal() : formatRate(option)}
                </Menu.RadioItem>
              ))}
            </Menu.RadioGroup>
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}

function formatRate(rate: number) {
  return `${rate}×`;
}
