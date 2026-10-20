import { Button } from "@tessera/ui/components/button";
import { Skeleton } from "@tessera/ui/components/skeleton";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@tessera/ui/components/tabs";
import { useTheme } from "@tessera/ui/components/theme-provider";
import { Check, Copy } from "lucide-react";
import { useEffect, useState } from "react";

import { MermaidDiagram } from "@/components/mermaid/mermaid-diagram";
import { SvgImage } from "@/components/svg-image";
import { copyText } from "@/lib/copy-text";
import * as m from "@/paraglide/messages";

const MAX_PREVIEW_BYTES = 200_000;
/** Languages that can be rendered: shown rendered by default, with a toggle back to the source. */
const RENDERABLE = new Set(["mermaid", "svg"]);
/** Languages without a shiki grammar of their own borrow a similar one's highlighting. */
const HIGHLIGHT_ALIASES: Record<string, string> = { svg: "xml" };

type PreviewState =
  | { status: "loading" }
  | { status: "too-large" }
  | { status: "error" }
  | { status: "ready"; code: string; html: string };

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
  const isRenderable = RENDERABLE.has(language ?? "");
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
        const lang = language || "plaintext";
        const html = await highlight(
          code,
          HIGHLIGHT_ALIASES[lang] ?? lang,
          theme
        );
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
    if (!(await copyText(state.code))) {
      return;
    }
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

  const copyButton = (
    <Button
      aria-label={m.share_copy()}
      onClick={handleCopy}
      size="icon-sm"
      type="button"
      variant="secondary"
    >
      {copied ? <Check /> : <Copy />}
    </Button>
  );

  const source = (
    <div className="group relative">
      {!isRenderable && (
        <div className="absolute top-2 right-2 z-10 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
          {copyButton}
        </div>
      )}
      {/* Shiki HTML-escapes the tokenized source when it serializes, so this
          cannot inject markup. That is what makes it safe here. */}
      <div
        className="max-h-96 overflow-auto rounded-md border border-border/60 text-xs [&_pre]:bg-transparent! [&_pre]:p-4"
        // oxlint-disable-next-line react/no-danger -- trusted output of the Shiki highlighter, not user-supplied HTML
        dangerouslySetInnerHTML={{ __html: state.html }}
      />
    </div>
  );

  if (!isRenderable) {
    return source;
  }

  return (
    <Tabs defaultValue="preview">
      <div className="flex items-center justify-between gap-2">
        <TabsList>
          <TabsTrigger value="preview">{m.code_tab_preview()}</TabsTrigger>
          <TabsTrigger value="source">{m.code_tab_edit()}</TabsTrigger>
        </TabsList>
        {copyButton}
      </div>
      <TabsContent keepMounted value="preview">
        <RenderedPreview code={state.code} language={language ?? ""} />
      </TabsContent>
      <TabsContent keepMounted value="source">
        {source}
      </TabsContent>
    </Tabs>
  );
}

function RenderedPreview({
  code,
  language,
}: {
  code: string;
  language: string;
}) {
  if (language === "svg") {
    // Always through SvgImage (blob + <img>), never into the DOM: scripts in an SVG would run.
    return (
      <SvgImage alt={m.code_tab_preview()} className="max-h-96" source={code} />
    );
  }
  return (
    <MermaidDiagram className="h-96" code={code} title={m.mermaid_diagram()} />
  );
}

async function highlight(code: string, language: string, theme: string) {
  const { codeToHtml } = await import("@/lib/shiki");
  try {
    return await codeToHtml(code, { lang: language, theme });
  } catch {
    return await codeToHtml(code, { lang: "text", theme });
  }
}
