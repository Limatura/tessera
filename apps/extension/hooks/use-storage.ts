import { useEffect, useState } from "react";

import type { WxtStorageItem } from "#imports";
import { listJobs, watchJobs } from "@/lib/jobs";
import type { Job } from "@/lib/storage";

/**
 * A storage item's value, kept current as any context changes it.
 * `undefined` until the first read comes back.
 */
export function useStorageItem<T>(
  item: WxtStorageItem<T, Record<string, unknown>>
): T | undefined {
  const [value, setValue] = useState<T>();
  useEffect(() => {
    let mounted = true;
    const load = async () => {
      try {
        const initial = await item.getValue();
        if (mounted) {
          setValue(initial);
        }
      } catch {
        // Storage unavailable while the extension reloads; the watch catches up.
      }
    };
    void load();
    const unwatch = item.watch((next) => setValue(next));
    return () => {
      mounted = false;
      unwatch();
    };
  }, [item]);
  return value;
}

/** Every upload in this browser session, newest first. */
export function useJobs(): Job[] {
  const [jobs, setJobs] = useState<Job[]>([]);
  useEffect(() => {
    let mounted = true;
    const load = async () => {
      try {
        const initial = await listJobs();
        if (mounted) {
          setJobs(initial);
        }
      } catch {
        // As above.
      }
    };
    void load();
    const unwatch = watchJobs(setJobs);
    return () => {
      mounted = false;
      unwatch();
    };
  }, []);
  return jobs;
}
