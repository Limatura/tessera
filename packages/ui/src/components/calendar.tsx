import { IconChevronLeft, IconChevronRight } from "@tabler/icons-react";
import { cn } from "@tessera/ui/lib/utils";
import { DayPicker } from "react-day-picker";

function Calendar({
  className,
  classNames,
  showOutsideDays = true,
  ...props
}: React.ComponentProps<typeof DayPicker>) {
  return (
    <DayPicker
      showOutsideDays={showOutsideDays}
      className={cn("w-full", className)}
      classNames={{
        button_next:
          "inline-flex size-8 items-center justify-center rounded-md hover:bg-muted",
        button_previous:
          "inline-flex size-8 items-center justify-center rounded-md hover:bg-muted",
        caption_label: "text-sm font-medium",
        day: "p-0 text-center",
        day_button:
          "inline-flex size-9 items-center justify-center rounded-md text-sm tabular-nums outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring/50 sm:size-8",
        disabled: "opacity-40",
        hidden: "invisible",
        month: "flex w-full flex-col gap-2",
        month_caption: "flex h-8 items-center justify-center",
        month_grid: "w-full border-collapse",
        months: "relative flex flex-col",
        nav: "absolute inset-x-0 top-0 flex h-8 items-center justify-between",
        outside: "text-muted-foreground opacity-60",
        root: "w-full",
        selected:
          "[&>button]:bg-primary [&>button]:text-primary-foreground [&>button:hover]:bg-primary",
        today: "[&>button]:ring-1 [&>button]:ring-border",
        week: "",
        weekday: "pb-1 text-xs font-normal text-muted-foreground",
        ...classNames,
      }}
      components={{
        Chevron: ({ orientation }) =>
          orientation === "left" ? (
            <IconChevronLeft className="size-4" />
          ) : (
            <IconChevronRight className="size-4" />
          ),
      }}
      {...props}
    />
  );
}

export { Calendar };
