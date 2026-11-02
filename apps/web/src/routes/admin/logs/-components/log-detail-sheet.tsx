import { Badge } from "@tessera/ui/components/badge";
import {
  Drawer,
  DrawerContent,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
} from "@tessera/ui/components/drawer";
import { ScrollArea } from "@tessera/ui/components/scroll-area";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@tessera/ui/components/sheet";
import { useIsMobile } from "@tessera/ui/hooks/use-mobile";
import { cn } from "@tessera/ui/lib/utils";

import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import type { LogEntry } from "@/lib/admin/log-entries";
import * as m from "@/paraglide/messages";

import { CopyButton } from "./copy-button";
import { LEVEL_STYLES_TEXT, formatValue, lineAsJson } from "./log-row";

interface LogDetailSheetProps {
  entry: LogEntry | null;
  onClose: () => void;
}

/**
 * One whole line: the message unclipped, every field, and ways to copy it. A
 * bottom drawer on a phone, where the buttons sit under the thumb and a swipe
 * closes it; a side sheet from `md` up, so the list stays visible beside it.
 */
export function LogDetailSheet({ entry, onClose }: LogDetailSheetProps) {
  const isMobile = useIsMobile();
  const handleOpenChange = (open: boolean) => {
    if (!open) {
      onClose();
    }
  };

  if (isMobile) {
    return (
      <Drawer
        onOpenChange={handleOpenChange}
        open={entry !== null}
        showSwipeHandle
      >
        <DrawerContent>
          {entry && (
            <>
              <DrawerHeader>
                <DrawerTitle className="flex items-center gap-2">
                  <LevelAndTime entry={entry} />
                </DrawerTitle>
                <DrawerDescription className="font-mono text-left text-xs">
                  {entry.time}
                </DrawerDescription>
              </DrawerHeader>
              <div className="max-h-[60svh] overflow-y-auto overscroll-contain px-4 mt-4">
                <LogDetailBody entry={entry} />
              </div>
              <DrawerFooter className="flex-row justify-end pb-[max(1rem,env(safe-area-inset-bottom))]">
                <LogDetailActions entry={entry} />
              </DrawerFooter>
            </>
          )}
        </DrawerContent>
      </Drawer>
    );
  }

  return (
    <Sheet onOpenChange={handleOpenChange} open={entry !== null}>
      <SheetContent className="w-full sm:max-w-xl">
        {entry && (
          <>
            <SheetHeader className="pr-14">
              <SheetTitle className="flex items-center gap-2">
                <LevelAndTime entry={entry} />
              </SheetTitle>
              <SheetDescription className="font-mono text-xs">
                {entry.time}
              </SheetDescription>
            </SheetHeader>
            <ScrollArea className="min-h-0 flex-1">
              <div className="px-6 pb-4">
                <LogDetailBody entry={entry} />
              </div>
            </ScrollArea>
            <SheetFooter className="flex-row justify-end">
              <LogDetailActions entry={entry} />
            </SheetFooter>
          </>
        )}
      </SheetContent>
    </Sheet>
  );
}

function LevelAndTime({ entry }: { entry: LogEntry }) {
  const formatDateTime = useDateTimeFormatter();
  return (
    <>
      <Badge
        className={cn("uppercase", LEVEL_STYLES_TEXT[entry.level])}
        variant="outline"
      >
        {entry.level}
      </Badge>
      {formatDateTime(entry.time)}
    </>
  );
}

function LogDetailBody({ entry }: { entry: LogEntry }) {
  const fields = Object.entries(entry.fields);

  return (
    <div className="flex flex-col gap-5">
      <Section title={m.logs_detail_target()}>
        <ValueBox copy={entry.target} label={m.logs_detail_target()}>
          {entry.target}
        </ValueBox>
      </Section>
      <Section title={m.logs_detail_message()}>
        <ValueBox
          className="whitespace-pre-wrap break-words"
          copy={entry.message}
          label={m.logs_detail_message()}
        >
          {entry.message}
        </ValueBox>
      </Section>
      {fields.length > 0 && (
        <Section title={m.logs_detail_fields()}>
          <div className="flex flex-col gap-2">
            {fields.map(([key, value]) => (
              <FieldBox key={key} name={key} value={value} />
            ))}
          </div>
        </Section>
      )}
      <Section title={m.logs_detail_id()}>
        <ValueBox
          className="text-muted-foreground"
          copy={entry.id}
          label={m.logs_detail_id()}
        >
          {entry.id}
        </ValueBox>
      </Section>
    </div>
  );
}

/** A value in a tinted box, with its own copy button in the corner. */
function ValueBox({
  children,
  copy,
  label,
  className,
}: {
  children: React.ReactNode;
  copy: string;
  label: string;
  className?: string;
}) {
  return (
    <div className="relative rounded-md border bg-muted/40">
      <p
        className={cn(
          "break-all p-2.5 pr-10 font-mono text-xs leading-relaxed",
          className
        )}
      >
        {children}
      </p>
      <CopyButton
        className="absolute top-1 right-1"
        label={label}
        text={() => copy}
      />
    </div>
  );
}

function FieldBox({ name, value }: { name: string; value: unknown }) {
  const text = formatValue(value);
  return (
    <div className="relative rounded-md border bg-muted/40">
      <div className="flex flex-col gap-0.5 p-2.5 pr-10 font-mono text-xs">
        <span className="text-muted-foreground">{name}</span>
        <span className="whitespace-pre-wrap break-all">{text}</span>
      </div>
      <CopyButton
        className="absolute top-1 right-1"
        label={name}
        text={() => text}
      />
    </div>
  );
}

function LogDetailActions({ entry }: { entry: LogEntry }) {
  return (
    <CopyButton
      label={m.logs_copy_json()}
      text={() => lineAsJson(entry)}
      withLabel
    />
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-1.5">
      <h3 className="font-medium text-muted-foreground text-xs">{title}</h3>
      {children}
    </section>
  );
}
