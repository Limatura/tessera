import { useQuery } from "@tanstack/react-query";
import type { components } from "@tessera/api/schema";
import { ShareKey, openBytes, ready } from "@tessera/crypto";
import { Button } from "@tessera/ui/components/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { toast } from "@tessera/ui/components/toast";
import { formatBytes } from "@tessera/ui/lib/format";
import {
  Download,
  Eye,
  FileIcon,
  FileText,
  Flame,
  KeyRound,
  Link2Off,
  LockKeyhole,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { useShareTicket } from "@/hooks/use-share";
import { buildTransferUrl } from "@/lib/api/share";
import type { OpenedFile, OpenedShare } from "@/lib/download/encrypted";
import {
  baseName,
  openShare,
  openedFileBlob,
  openedFileStream,
} from "@/lib/download/encrypted";
import { canSaveStreams, openSaveTarget } from "@/lib/download/save-stream";
import { zipLength, zipStream } from "@/lib/download/zip";
import * as m from "@/paraglide/messages";

import { resolveFilePreview } from "./file-previews/registry";
import type { FilePreviewComponent } from "./file-previews/types";
import { ShareMeta } from "./share-meta";
import { StatusCard } from "./status-card";
import { TextPreview } from "./text-preview";

type PublicShare = components["schemas"]["PublicShareResponse"];

/**
 * Past this a preview would hold too much in memory: the whole file is opened
 * before it can be shown, unlike a download, which streams.
 */
const MAX_PREVIEW_BYTES = 20 * 1024 * 1024;

/**
 * An end-to-end encrypted share. Everything the server cannot read is
 * opened here: the title, file names, a note, and each file as it downloads.
 * Several files are packed into a zip here too, since the server will not
 * archive sealed files.
 *
 * `passwordKey` is the key a password derived, for `encryption=password`.
 * Otherwise the key is the link's fragment, which never reached the server.
 */
export function EncryptedShare({
  code,
  data,
  password,
  passwordKey,
}: {
  code: string;
  data: PublicShare;
  password?: string;
  passwordKey?: ShareKey;
}) {
  const wasm = useQuery({
    queryFn: async () => {
      await ready();
      return true;
    },
    queryKey: ["crypto-ready"],
    staleTime: Number.POSITIVE_INFINITY,
  });

  const opened = useMemo(() => {
    if (!wasm.data) {
      return;
    }
    return openWithKey(data, passwordKey);
  }, [wasm.data, data, passwordKey]);

  if (wasm.isError) {
    return (
      <StatusCard
        description={m.share_decrypt_failed()}
        icon={<LockKeyhole className="size-5" />}
        title={m.share_bad_key_title()}
      />
    );
  }
  if (!opened) {
    return <Skeleton className="h-48 w-full max-w-xl rounded-xl" />;
  }
  if (opened.status === "missing") {
    return (
      <StatusCard
        description={m.share_needs_full_link_description()}
        icon={<Link2Off className="size-5" />}
        title={m.share_needs_full_link_title()}
      />
    );
  }
  if (opened.status === "wrong") {
    return (
      <StatusCard
        description={m.share_bad_key_description()}
        icon={<KeyRound className="size-5" />}
        title={m.share_bad_key_title()}
      />
    );
  }

  return (
    <OpenedShareCard
      code={code}
      data={data}
      opened={opened.share}
      password={password}
      shareKey={opened.key}
    />
  );
}

type Opening =
  | { status: "missing" }
  | { status: "wrong" }
  | { status: "open"; key: ShareKey; share: OpenedShare };

/** The key and what it opens, or why there is nothing to open. */
function openWithKey(data: PublicShare, passwordKey?: ShareKey): Opening {
  let key = passwordKey;
  if (!key) {
    const fragment = window.location.hash.slice(1);
    if (!fragment) {
      return { status: "missing" };
    }
    try {
      key = ShareKey.parse(fragment);
    } catch {
      return { status: "wrong" };
    }
  }
  try {
    return { key, share: openShare(key, data), status: "open" };
  } catch {
    return { status: "wrong" };
  }
}

function OpenedShareCard({
  code,
  data,
  opened,
  password,
  shareKey,
}: {
  code: string;
  data: PublicShare;
  opened: OpenedShare;
  password?: string;
  shareKey: ShareKey;
}) {
  const isText = data.kind === "text";
  const plainTotal = opened.files.reduce(
    (total, file) => total + file.plainBytes,
    0
  );

  return (
    <div className="w-full max-w-xl animate-in fade-in duration-200">
      <Card className="w-full border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
        <CardHeader className="items-center text-center">
          <div className="mb-2 flex size-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
            {isText ? (
              <FileText className="size-5" />
            ) : (
              <FileIcon className="size-5" />
            )}
          </div>
          <CardTitle
            className="w-full min-w-0 truncate font-medium text-xl tracking-tight"
            title={openedTitle(opened, isText)}
          >
            {openedTitle(opened, isText)}
          </CardTitle>
          <ShareMeta
            expiresAt={data.expires_at}
            sizeBytes={isText ? null : plainTotal}
            usesRemaining={data.uses_remaining}
          />
          <p className="flex items-center gap-1 text-muted-foreground text-xs">
            <LockKeyhole className="size-3.5 text-green-500" />
            {m.share_encrypted_badge()}
          </p>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          {isText ? (
            <SealedTextBody
              code={code}
              language={opened.language}
              oneTime={data.uses_remaining === 1}
              password={password}
              shareKey={shareKey}
            />
          ) : (
            <SealedFilesBody
              code={code}
              oneTime={data.uses_remaining === 1}
              opened={opened}
              password={password}
              shareKey={shareKey}
            />
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function openedTitle(opened: OpenedShare, isText: boolean) {
  const [single] = opened.files;
  if (opened.title) {
    return opened.title;
  }
  if (opened.files.length === 1 && single) {
    return baseName(single.path);
  }
  return isText ? m.share_untitled_text() : m.share_untitled();
}

/**
 * A note. A one-time one waits for a click: fetching it spends the only
 * retrieval, the same as a plain share's.
 */
function SealedTextBody({
  code,
  language,
  oneTime,
  password,
  shareKey,
}: {
  code: string;
  language?: string;
  oneTime: boolean;
  password?: string;
  shareKey: ShareKey;
}) {
  const [requested, setRequested] = useState(false);
  const ticket = useShareTicket(code, password, !oneTime || requested);

  if (oneTime && !requested) {
    return (
      <div className="flex flex-col items-center gap-3 rounded-md border border-border/60 bg-muted/20 p-8 text-center">
        <Flame className="size-8 text-muted-foreground" />
        <p className="text-muted-foreground text-sm">
          {m.share_one_time_text()}
        </p>
        <Button onClick={() => setRequested(true)} type="button">
          <Eye />
          {m.share_view_text()}
        </Button>
      </div>
    );
  }
  return (
    <SealedText
      code={code}
      language={language}
      shareKey={shareKey}
      ticket={ticket.data?.ticket}
    />
  );
}

/** Which preview a file gets, if any. Kept in an object so the component is not made during render. */
function previewFor(
  file: OpenedFile | undefined,
  several: boolean,
  oneTime: boolean
) {
  if (!file || several || oneTime || file.plainBytes > MAX_PREVIEW_BYTES) {
    return null;
  }
  const Component = resolveFilePreview(baseName(file.path));
  return Component ? { Component, file } : null;
}

function SealedFilesBody({
  code,
  oneTime,
  opened,
  password,
  shareKey,
}: {
  code: string;
  oneTime: boolean;
  opened: OpenedShare;
  password?: string;
  shareKey: ShareKey;
}) {
  const [single] = opened.files;
  const several = opened.files.length > 1;
  const preview = previewFor(single, several, oneTime);
  const ticket = useShareTicket(code, password, preview !== null);
  const [busy, setBusy] = useState(false);

  // Spends the retrieval on the first download, not on page load
  const getTicket = async () => {
    const held = ticket.data?.ticket;
    if (held) {
      return held;
    }
    const fetched = await ticket.refetch();
    if (!fetched.data) {
      throw new Error("no ticket");
    }
    return fetched.data.ticket;
  };

  const handleDownloadFile = (file: OpenedFile) =>
    saveFile(
      baseName(file.path),
      file.plainBytes,
      () => openedFileStream(code, shareKey, file, getTicket),
      () => openedFileBlob(code, shareKey, file, getTicket),
      setBusy
    );

  const handleDownloadAll = () => {
    const entries = opened.files.map((file) => ({
      name: file.path,
      open: () => openedFileStream(code, shareKey, file, getTicket),
      size: file.plainBytes,
    }));
    const stream = () => Promise.resolve(zipStream(entries));
    return saveFile(
      `${opened.title || code}.zip`,
      zipLength(entries),
      stream,
      async () => {
        const zipped = await stream();
        return new Blob([await new Response(zipped).arrayBuffer()]);
      },
      setBusy
    );
  };

  return (
    <>
      {preview && (
        <SealedPreview
          code={code}
          file={preview.file}
          preview={preview}
          shareKey={shareKey}
          ticket={ticket.data?.ticket}
        />
      )}

      {several && (
        <ul className="flex max-h-64 flex-col divide-y divide-border/60 overflow-y-auto rounded-md border border-border/60">
          {opened.files.map((file) => (
            <li
              className="flex items-center justify-between gap-2 px-3 py-2"
              key={file.index}
            >
              <span className="min-w-0 truncate text-sm">{file.path}</span>
              <span className="flex shrink-0 items-center gap-2">
                <span className="text-muted-foreground text-xs">
                  {formatBytes(file.plainBytes)}
                </span>
                <Button
                  aria-label={m.share_download()}
                  disabled={busy}
                  onClick={() => handleDownloadFile(file)}
                  size="icon-sm"
                  type="button"
                  variant="ghost"
                >
                  <Download />
                </Button>
              </span>
            </li>
          ))}
        </ul>
      )}

      {!(preview || several) && (
        <div className="flex flex-col items-center gap-2 rounded-md border border-border/60 bg-muted/20 p-8 text-center">
          {oneTime ? (
            <Flame className="size-8 text-muted-foreground" />
          ) : (
            <FileIcon className="size-8 text-muted-foreground" />
          )}
          <p className="text-muted-foreground text-sm">
            {oneTime ? m.share_one_time_file() : m.share_no_preview()}
          </p>
        </div>
      )}

      {single && (
        <Button
          className="w-full"
          disabled={busy}
          onClick={() =>
            several ? handleDownloadAll() : handleDownloadFile(single)
          }
          type="button"
        >
          <Download />
          {downloadLabel(busy, several)}
        </Button>
      )}

      {busy && (
        <p className="text-center text-muted-foreground text-xs">
          {m.share_join_keep_open()}
        </p>
      )}
    </>
  );
}

/** A sealed note, fetched whole and opened here: `/raw` will not serve it. */
function SealedText({
  code,
  language,
  shareKey,
  ticket,
}: {
  code: string;
  language?: string;
  shareKey: ShareKey;
  ticket?: string;
}) {
  const text = useQuery({
    enabled: ticket !== undefined,
    queryFn: async () => {
      const response = await fetch(
        buildTransferUrl(code, "download", ticket as string)
      );
      if (!response.ok) {
        throw new Error(`download failed: ${response.status}`);
      }
      const sealed = new Uint8Array(await response.arrayBuffer());
      return new Blob([openBytes(shareKey, sealed) as BlobPart], {
        type: "text/plain;charset=utf-8",
      });
    },
    queryKey: ["sealed-text", code],
    staleTime: Number.POSITIVE_INFINITY,
  });
  const url = useObjectUrl(text.data);

  if (text.isError) {
    return (
      <p className="text-center text-destructive text-sm">
        {m.share_decrypt_failed()}
      </p>
    );
  }
  if (!(url && text.data)) {
    return <Skeleton className="h-48 w-full rounded-md" />;
  }
  return (
    <TextPreview language={language} sizeBytes={text.data.size} url={url} />
  );
}

/** One file opened into memory and shown, the way a plain share's preview is. */
function SealedPreview({
  code,
  file,
  preview,
  shareKey,
  ticket,
}: {
  code: string;
  file: OpenedFile;
  preview: { Component: FilePreviewComponent };
  shareKey: ShareKey;
  ticket?: string;
}) {
  const blob = useQuery({
    enabled: ticket !== undefined,
    queryFn: () =>
      openedFileBlob(code, shareKey, file, () =>
        Promise.resolve(ticket as string)
      ),
    queryKey: ["sealed-preview", code, file.index],
    staleTime: Number.POSITIVE_INFINITY,
  });
  const url = useObjectUrl(blob.data);
  const name = baseName(file.path);

  if (blob.isError) {
    return (
      <p className="text-center text-destructive text-sm">
        {m.share_decrypt_failed()}
      </p>
    );
  }
  if (!url) {
    return <Skeleton className="h-48 w-full rounded-md" />;
  }
  return <preview.Component filename={name} url={url} />;
}

function useObjectUrl(blob: Blob | undefined) {
  const [url, setUrl] = useState<string>();
  useEffect(() => {
    if (!blob) {
      return;
    }
    const next = URL.createObjectURL(blob);
    // The URL has to be revoked by the effect that made it, so it lives in
    // state rather than a memo that Strict Mode's second run would outlive.
    // oxlint-disable-next-line react/set-state-in-effect -- see the line above
    setUrl(next);
    return () => URL.revokeObjectURL(next);
  }, [blob]);
  return url;
}

/**
 * Streams to a file the visitor picks when the browser can, and otherwise
 * opens the whole thing in memory and hands it over as a blob. Called straight
 * from the click, before any other `await`, for the save picker's sake.
 */
async function saveFile(
  name: string,
  size: number,
  stream: () => Promise<ReadableStream<Uint8Array>>,
  blob: () => Promise<Blob>,
  setBusy: (busy: boolean) => void
) {
  try {
    if (canSaveStreams()) {
      const target = await openSaveTarget(name, size);
      if (!target) {
        return;
      }
      setBusy(true);
      let source: ReadableStream<Uint8Array>;
      try {
        source = await stream();
      } catch (error) {
        await target.cancel();
        throw error;
      }
      await target.write(source);
      return;
    }
    setBusy(true);
    downloadBlob(name, await blob());
  } catch (error) {
    if (!(error instanceof DOMException && error.name === "AbortError")) {
      toast.add({
        description: m.share_decrypt_failed(),
        title: m.share_join_failed_title(),
        type: "error",
      });
    }
  } finally {
    setBusy(false);
  }
}

function downloadBlob(name: string, blob: Blob) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

function downloadLabel(busy: boolean, several: boolean) {
  if (busy) {
    return m.share_decrypting();
  }
  return several ? m.share_download_all() : m.share_download();
}
