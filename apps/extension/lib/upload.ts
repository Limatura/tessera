/**
 * One upload, from what the user right-clicked (or typed into the popup) to a
 * link on the clipboard. Runs in the background and reports through
 * `jobs.ts`. Files picked by hand go through the site's own upload page.
 */

import type { ShareSettings, UploadPayload } from "@tessera/upload";

import { i18n } from "#i18n";

import { copyText } from "./clipboard";
import { connection, fetchCapabilities, uploaderFor } from "./instance";
import { saveJob } from "./jobs";
import { notify } from "./notify";
import { MediaError, fetchMedia } from "./page-content";
import type { ClickedIn } from "./page-content";
import { defaultsItem, keepShare } from "./storage";
import type { Job } from "./storage";

export type UploadSource =
  | { type: "media"; url: string; clickedIn?: ClickedIn }
  | { type: "note"; text: string; title?: string; language?: string }
  | { type: "link"; url: string };

/** A failure whose message is already fit to show. */
class UploadError extends Error {
  override name = "UploadError";
}

/** How often progress is written at most; storage is not a progress bar. */
const PROGRESS_INTERVAL_MS = 250;

const MINUTES_PER_HOUR = 60;

const controllers = new Map<string, AbortController>();

/** Stops a running upload; nothing when it has already finished. */
export function cancelUpload(id: string): void {
  controllers.get(id)?.abort();
}

export async function runUpload(source: UploadSource): Promise<Job> {
  let job: Job = {
    id: crypto.randomUUID(),
    kind: kindOf(source),
    name: nameOf(source),
    progress: 0,
    startedAt: Date.now(),
    state: "preparing",
  };
  const update = async (patch: Partial<Job>) => {
    job = { ...job, ...patch };
    await saveJob(job);
  };
  await saveJob(job);

  const controller = new AbortController();
  controllers.set(job.id, controller);
  try {
    const link = await upload(source, controller.signal, update);
    await update({ link, progress: 100, state: "done" });
    await announceDone(job, link);
  } catch (error) {
    if (controller.signal.aborted) {
      await update({ state: "cancelled" });
    } else {
      const message = describe(error);
      await update({ error: message, state: "failed" });
      await announceFailure(job, message);
    }
  } finally {
    controllers.delete(job.id);
  }
  return job;
}

async function upload(
  source: UploadSource,
  signal: AbortSignal,
  update: (patch: Partial<Job>) => Promise<void>
): Promise<string> {
  const current = await connection();
  if (!current) {
    throw new UploadError(i18n.t("errors.noServer"));
  }
  const { server, credential } = current;
  const [defaults, capabilities] = await Promise.all([
    defaultsItem.getValue(),
    fetchCapabilities(server, credential),
  ]);
  if (!credential && !capabilities.anonymous_upload) {
    throw new UploadError(i18n.t("errors.signInRequired"));
  }

  const payload = await toPayload(source, signal);
  if (payload.type === "file" && payload.files.length === 1) {
    await update({ name: payload.files[0]?.file.name });
  }

  // A link share is a redirect the server has to read, so it is never
  // sealed. The sealing module loads itself on first use (`prepareSealing`).
  const encrypt =
    payload.type !== "link" && capabilities.encryption && defaults.encrypt;

  await update({ state: "uploading" });
  const uploader = uploaderFor(server, credential);
  let lastWrite = 0;
  const result = await uploader.upload(payload, {
    encryption: encrypt ? { mode: "link" } : undefined,
    onProgress: (percent) => {
      const now = Date.now();
      if (now - lastWrite >= PROGRESS_INTERVAL_MS) {
        lastWrite = now;
        update({ progress: Math.min(99, percent) }).catch(() => {
          // A missed progress write changes nothing but the bar.
        });
      }
    },
    signal,
  });

  const settings = shareSettings(
    defaults.expiresInHours,
    defaults.burnAfterReading,
    capabilities.max_expiry_hours ?? null
  );
  if (Object.keys(settings).length > 0) {
    try {
      await uploader.updateShare(result.code, settings, result.managementToken);
    } catch (error) {
      // A share without the limit asked for (burn after reading, say) is not
      // the share the user meant to hand out, so it does not stay.
      await uploader
        .deleteShare(result.code, result.managementToken)
        .catch(() => {
          // The original error is the one worth reporting.
        });
      throw error;
    }
  }

  await keepShare({
    code: result.code,
    createdAt: Date.now(),
    encrypted: result.encrypted ?? false,
    kind: kindOf(source),
    link: result.shortUrl,
    managementToken: result.managementToken,
    // A note without a title would be named by its code, which says
    // nothing; the first line it started with does.
    name: result.fileName === result.code ? nameOf(source) : result.fileName,
    server,
  });
  return result.shortUrl;
}

function shareSettings(
  expiresInHours: number | null,
  burnAfterReading: boolean,
  maxExpiryHours: number | null
): ShareSettings {
  const settings: ShareSettings = {};
  if (expiresInHours !== null) {
    // `0` is "never expires". A default saved before the instance set or
    // lowered its ceiling would only be refused, so it is held to the ceiling.
    const hours =
      maxExpiryHours === null
        ? expiresInHours
        : Math.min(expiresInHours || maxExpiryHours, maxExpiryHours);
    settings.expiresInMinutes = hours * MINUTES_PER_HOUR;
  }
  if (burnAfterReading) {
    settings.maxUses = 1;
  }
  return settings;
}

async function toPayload(
  source: UploadSource,
  signal: AbortSignal
): Promise<UploadPayload> {
  switch (source.type) {
    case "media": {
      const file = await fetchMedia(source.url, source.clickedIn, signal);
      return { files: [{ file, path: file.name }], type: "file" };
    }
    case "note": {
      return {
        content: source.text,
        language: source.language,
        title: source.title,
        type: "paste",
      };
    }
    case "link": {
      return { type: "link", url: source.url };
    }
    default: {
      const exhaustive: never = source;
      throw new Error(`unknown source ${String(exhaustive)}`);
    }
  }
}

function kindOf(source: UploadSource): Job["kind"] {
  switch (source.type) {
    case "note": {
      return "note";
    }
    case "link": {
      return "link";
    }
    default: {
      return "file";
    }
  }
}

const NAME_LENGTH = 60;

function nameOf(source: UploadSource): string {
  switch (source.type) {
    case "media": {
      return shorten(source.url);
    }
    case "note": {
      return (
        source.title ||
        shorten(source.text.trim().split("\n")[0] ?? "") ||
        i18n.t("jobs.note")
      );
    }
    case "link": {
      return shorten(source.url);
    }
    default: {
      return "";
    }
  }
}

function shorten(text: string): string {
  const chars = [...text];
  return chars.length > NAME_LENGTH
    ? `${chars.slice(0, NAME_LENGTH - 1).join("")}…`
    : text;
}

/** What went wrong, in words for the user. */
export function describe(error: unknown): string {
  if (error instanceof MediaError) {
    switch (error.reason) {
      case "blocked": {
        return i18n.t("errors.mediaBlocked");
      }
      case "too-large": {
        return i18n.t("errors.mediaTooLarge");
      }
      case "stream": {
        return i18n.t("errors.mediaStream");
      }
      default: {
        return i18n.t("errors.mediaHttp", [error.message]);
      }
    }
  }
  if (error instanceof TypeError) {
    // `fetch` says only "Failed to fetch": the instance is down or unreachable.
    return i18n.t("errors.network");
  }
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return i18n.t("errors.unknown");
}

async function announceDone(job: Job, link: string): Promise<void> {
  const defaults = await defaultsItem.getValue();
  let copied = false;
  if (defaults.copyLink) {
    copied = await copyText(link).then(
      () => true,
      () => false
    );
  }
  if (defaults.notify) {
    await notify(
      i18n.t("notify.doneTitle"),
      copied
        ? i18n.t("notify.doneCopied", [job.name])
        : i18n.t("notify.done", [job.name]),
      link
    ).catch(() => {
      // Notifications turned off at the system level.
    });
  }
}

async function announceFailure(job: Job, message: string): Promise<void> {
  const defaults = await defaultsItem.getValue();
  if (defaults.notify) {
    await notify(i18n.t("notify.failedTitle", [job.name]), message).catch(
      () => {
        // Notifications turned off at the system level.
      }
    );
  }
}
