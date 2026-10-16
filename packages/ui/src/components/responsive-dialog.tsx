"use client";

import { Drawer as DrawerPrimitive } from "@base-ui/react/drawer";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@tessera/ui/components/alert-dialog";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@tessera/ui/components/dialog";
import {
  Drawer,
  DrawerContent,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
} from "@tessera/ui/components/drawer";
import { useIsMobile } from "@tessera/ui/hooks/use-mobile";
import { cn } from "@tessera/ui/lib/utils";
import * as React from "react";

type Mode = "alert" | "dialog" | "drawer";

const ModeContext = React.createContext<Mode | null>(null);

function useMode() {
  const mode = React.useContext(ModeContext);
  if (!mode) {
    throw new Error(
      "ResponsiveDialog parts must be used within a ResponsiveDialog."
    );
  }
  return mode;
}

/**
 * A centred dialog from `md` up, a bottom drawer below it: on a phone the
 * buttons sit under the thumb, and a long form scrolls instead of being pushed
 * off screen by the keyboard.
 *
 * `alert` picks AlertDialog on desktop, for confirmations that must not close
 * on an outside click. The drawer always ignores outside presses in that mode
 * too; it can still be swiped away, which is a deliberate gesture.
 *
 * Crossing the breakpoint while open (rotating a tablet) swaps the component
 * and remounts the content, so unsaved input in it is lost.
 */
function ResponsiveDialog({
  open,
  onOpenChange,
  alert = false,
  disablePointerDismissal,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  alert?: boolean;
  disablePointerDismissal?: boolean;
  children: React.ReactNode;
}) {
  const isMobile = useIsMobile();
  const mode: Mode = isMobile ? "drawer" : alert ? "alert" : "dialog";
  const handleOpenChange = (next: boolean) => onOpenChange(next);

  let root: React.ReactNode;
  if (mode === "drawer") {
    root = (
      <Drawer
        disablePointerDismissal={alert || disablePointerDismissal}
        onOpenChange={handleOpenChange}
        open={open}
        showSwipeHandle
      >
        {children}
      </Drawer>
    );
  } else if (mode === "alert") {
    root = (
      <AlertDialog onOpenChange={handleOpenChange} open={open}>
        {children}
      </AlertDialog>
    );
  } else {
    root = (
      <Dialog
        disablePointerDismissal={disablePointerDismissal}
        onOpenChange={handleOpenChange}
        open={open}
      >
        {children}
      </Dialog>
    );
  }

  return <ModeContext.Provider value={mode}>{root}</ModeContext.Provider>;
}

/**
 * `className` styles the desktop popup only (width, `max-h`, ...): those sizes
 * mean nothing on a full-width drawer. The drawer body scrolls as a whole, and
 * keeps clear of the home indicator.
 */
function ResponsiveDialogContent({
  className,
  drawerClassName,
  children,
}: {
  className?: string;
  drawerClassName?: string;
  children: React.ReactNode;
}) {
  const mode = useMode();

  if (mode === "drawer") {
    return (
      <DrawerContent>
        <DrawerPrimitive.VirtualKeyboardProvider>
          <div
            className={cn(
              "flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto overscroll-contain px-4 pt-2 pb-[max(1rem,env(safe-area-inset-bottom))]",
              drawerClassName
            )}
          >
            {children}
          </div>
        </DrawerPrimitive.VirtualKeyboardProvider>
      </DrawerContent>
    );
  }
  if (mode === "alert") {
    return (
      <AlertDialogContent className={className}>{children}</AlertDialogContent>
    );
  }
  return <DialogContent className={className}>{children}</DialogContent>;
}

function ResponsiveDialogHeader({
  className,
  ...props
}: React.ComponentProps<"div">) {
  const mode = useMode();
  if (mode === "drawer") {
    // The drawer's header pads itself; the body here already does.
    return (
      <DrawerHeader
        className={cn(
          "p-0 text-left group-data-[swipe-axis=y]/drawer-popup:text-left",
          className
        )}
        {...props}
      />
    );
  }
  if (mode === "alert") {
    return <AlertDialogHeader className={className} {...props} />;
  }
  return <DialogHeader className={className} {...props} />;
}

/** Buttons stack full width on a phone, the primary one on top. */
function ResponsiveDialogFooter({
  className,
  ...props
}: React.ComponentProps<"div">) {
  const mode = useMode();
  if (mode === "drawer") {
    return (
      <DrawerFooter
        className={cn(
          "mt-auto flex-col-reverse p-0 pt-2 *:data-[slot=button]:h-10",
          className
        )}
        {...props}
      />
    );
  }
  if (mode === "alert") {
    return <AlertDialogFooter className={className} {...props} />;
  }
  return <DialogFooter className={className} {...props} />;
}

function ResponsiveDialogTitle({
  className,
  ...props
}: React.ComponentProps<typeof DialogTitle>) {
  const mode = useMode();
  if (mode === "drawer") {
    return <DrawerTitle className={cn("text-base", className)} {...props} />;
  }
  if (mode === "alert") {
    return <AlertDialogTitle className={className} {...props} />;
  }
  return <DialogTitle className={className} {...props} />;
}

function ResponsiveDialogDescription({
  className,
  ...props
}: React.ComponentProps<typeof DialogDescription>) {
  const mode = useMode();
  if (mode === "drawer") {
    return (
      <DrawerDescription
        className={cn("text-sm text-pretty", className)}
        {...props}
      />
    );
  }
  if (mode === "alert") {
    return <AlertDialogDescription className={className} {...props} />;
  }
  return <DialogDescription className={className} {...props} />;
}

export {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
};
