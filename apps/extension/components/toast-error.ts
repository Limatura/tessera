import { toast } from "@tessera/ui/components/toast";

/** A failure as a toast: what was being tried, and what the error said. */
export function toastError(title: string, error: unknown): void {
  toast.add({
    description: error instanceof Error ? error.message : String(error),
    title,
    type: "error",
  });
}
