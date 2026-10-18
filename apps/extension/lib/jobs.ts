/**
 * Uploads in progress, in `storage.session`, one key per upload.
 *
 * One key each, not one array: several uploads run at once, and each writing
 * the whole array back would overwrite the progress another just saved.
 */
import { browser } from "#imports";

import type { Job } from "./storage";

const PREFIX = "job:";

/** How many finished uploads stay listed before the oldest are dropped. */
const KEEP_FINISHED = 20;

export function isActive(job: Job): boolean {
  return job.state === "preparing" || job.state === "uploading";
}

export async function listJobs(): Promise<Job[]> {
  const all = await browser.storage.session.get(null);
  return Object.entries(all)
    .filter(([key]) => key.startsWith(PREFIX))
    .map(([, value]) => value as Job)
    .toSorted((a, b) => b.startedAt - a.startedAt);
}

export async function saveJob(job: Job): Promise<void> {
  await browser.storage.session.set({ [PREFIX + job.id]: job });
}

export async function removeJobs(ids: string[]): Promise<void> {
  await browser.storage.session.remove(ids.map((id) => PREFIX + id));
}

/** Drops finished uploads past the newest {@link KEEP_FINISHED}. */
export async function pruneJobs(): Promise<void> {
  const jobs = await listJobs();
  const finished = jobs.filter((job) => !isActive(job));
  await removeJobs(finished.slice(KEEP_FINISHED).map((job) => job.id));
}

/** Calls `listener` with every job whenever any of them changes. */
export function watchJobs(listener: (jobs: Job[]) => void): () => void {
  const handleChange = (
    changes: Record<string, unknown>,
    area: string
  ): void => {
    if (
      area === "session" &&
      Object.keys(changes).some((key) => key.startsWith(PREFIX))
    ) {
      void notifyJobs(listener);
    }
  };
  browser.storage.onChanged.addListener(handleChange);
  return () => browser.storage.onChanged.removeListener(handleChange);
}

async function notifyJobs(listener: (jobs: Job[]) => void) {
  try {
    listener(await listJobs());
  } catch {
    // Storage gone (the extension is being reloaded): nothing to show.
  }
}

/**
 * Fails the jobs a stopped service worker left running: the request went
 * with it, so they would otherwise show as moving forever. Only jobs from
 * before `startedBefore`: the click that woke the worker may already have
 * started a new one.
 */
export async function abandonJobs(startedBefore: number, reason: string) {
  const jobs = await listJobs();
  await Promise.all(
    jobs
      .filter((job) => isActive(job) && job.startedAt < startedBefore)
      .map((job) => saveJob({ ...job, error: reason, state: "failed" }))
  );
}
