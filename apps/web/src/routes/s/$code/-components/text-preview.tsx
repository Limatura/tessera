import { Button } from "@tessera/ui/components/button";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { Check, Copy } from "lucide-react";
import { useEffect, useState } from "react";

import { useTheme } from "@/components/theme-provider";
import * as m from "@/paraglide/messages";

const MAX_PREVIEW_BYTES = 200_000;

type PreviewState =
  | { status: "loading" }
  | { status: "too-large" }
  | { status: "error" }
  | { status: "ready"; code: string; html: string };

async function highlight(code: string, language: string, theme: string) {
  const { codeToHtml } = await import("@/lib/shiki");
  try {
    return await codeToHtml(code, { lang: language, theme });
  } catch {
    return await codeToHtml(code, { lang: "text", theme });
  }
}

export function TextPreview({
  language,
  sizeBytes,
  url,
}: {
  language?: string | null;
  sizeBytes?: number | null;
  url: string;
}) {
  const { resolvedTheme } = useTheme();
  const tooLarge =
    sizeBytes !== null &&
    sizeBytes !== undefined &&
    sizeBytes > MAX_PREVIEW_BYTES;
  const [state, setState] = useState<PreviewState>(
    tooLarge ? { status: "too-large" } : { status: "loading" }
  );
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (tooLarge) {
      return;
    }

    const controller = new AbortController();
    const theme = resolvedTheme === "dark" ? "dark-plus" : "light-plus";

    (async () => {
      try {
        const response = await fetch(url, { signal: controller.signal });
        if (!response.ok) {
          throw new Error(response.statusText);
        }
        const code = await response.text();
        const html = await highlight(code, language || "plaintext", theme);
        setState({ code, html, status: "ready" });
      } catch {
        if (!controller.signal.aborted) {
          setState({ status: "error" });
        }
      }
    })();

    return () => controller.abort();
  }, [url, language, resolvedTheme, tooLarge]);

  const handleCopy = async () => {
    if (state.status !== "ready") {
      return;
    }
    await navigator.clipboard.writeText(state.code);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  if (state.status === "loading") {
    return <Skeleton className="h-48 w-full rounded-md" />;
  }

  if (state.status === "too-large" || state.status === "error") {
    return (
      <p className="rounded-md border border-border/60 bg-muted/30 p-6 text-center text-muted-foreground text-sm">
        {state.status === "too-large"
          ? m.share_preview_too_large()
          : m.share_preview_failed()}
      </p>
    );
  }

  return (
    <div className="group relative">
      <Button
        aria-label={m.share_copy()}
        className="absolute top-2 right-2 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"
        onClick={handleCopy}
        size="icon-sm"
        type="button"
        variant="secondary"
      >
        {copied ? <Check /> : <Copy />}
      </Button>
      {/* Shiki HTML-escapes the tokenized source when it serializes, so this
          can't be used to inject markup — that's what makes it safe here. */}
      <div
        className="max-h-96 overflow-auto rounded-md border border-border/60 text-xs [&_pre]:bg-transparent! [&_pre]:p-4"
        // oxlint-disable-next-line react/no-danger -- trusted output of the Shiki highlighter, not user-supplied HTML
        dangerouslySetInnerHTML={{ __html: state.html }}
      />
    </div>
  );
}
