import { IconCalendar } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import { Calendar } from "@tessera/ui/components/calendar";
import { Input } from "@tessera/ui/components/input";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@tessera/ui/components/popover";
import { cn } from "@tessera/ui/lib/utils";

import { getLocale } from "@/lib/locale";

const DEFAULT_TIME = "00:00";

interface DateTimePickerProps {
  /** An RFC 3339 time, or `null` for none. */
  value: string | null;
  onChange: (value: string | null) => void;
  /** Names the control and stands in as its placeholder. */
  label: string;
  className?: string;
}

/** A date from a calendar and a time of day, in local time, as one RFC 3339 value. */
export function DateTimePicker({
  value,
  onChange,
  label,
  className,
}: DateTimePickerProps) {
  const date = value ? new Date(value) : undefined;
  const time = date ? timeOf(date) : DEFAULT_TIME;

  const handleDay = (day: Date | undefined) => {
    if (!day) {
      onChange(null);
      return;
    }
    onChange(combine(day, time));
  };

  const handleTime = (next: string) => {
    if (next) {
      onChange(combine(date ?? new Date(), next));
    }
  };

  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            aria-label={label}
            className={cn(
              "justify-start font-normal",
              !date && "text-muted-foreground",
              className
            )}
            type="button"
            variant="outline"
          >
            <IconCalendar />
            <span className="truncate tabular-nums">
              {date ? format(date) : label}
            </span>
          </Button>
        }
      />
      <PopoverContent>
        <Calendar
          defaultMonth={date}
          mode="single"
          onSelect={handleDay}
          required={false}
          selected={date}
        />
        <Input
          aria-label={label}
          className="mt-2 h-9 w-full"
          onChange={(event) => handleTime(event.target.value)}
          type="time"
          value={time}
        />
      </PopoverContent>
    </Popover>
  );
}

function pad(n: number) {
  return String(n).padStart(2, "0");
}

function timeOf(date: Date) {
  return `${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function combine(day: Date, time: string) {
  const [hours = 0, minutes = 0] = time.split(":").map(Number);
  const next = new Date(day);
  next.setHours(hours, minutes, 0, 0);
  return next.toISOString();
}

function format(date: Date) {
  return date.toLocaleString(getLocale(), {
    dateStyle: "medium",
    timeStyle: "short",
  });
}
