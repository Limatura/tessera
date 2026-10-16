import { Button } from "@tessera/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@tessera/ui/components/dialog";
import { Label } from "@tessera/ui/components/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@tessera/ui/components/tabs";
import { ExternalChange } from "@uiw/react-codemirror";
import type { Extension, ViewUpdate } from "@uiw/react-codemirror";
import { Maximize2, Sparkles } from "lucide-react";
import { useEffect, useReducer, useRef, useState } from "react";

import { CodeEditor } from "@/components/code-editor";
import {
  EDITOR_DEBOUNCE_MS,
  MermaidDiagram,
} from "@/components/mermaid/mermaid-diagram";
import { SvgImage } from "@/components/svg-image";
import { detectLanguage } from "@/lib/detect-language";
import { LANGUAGE_LABELS } from "@/lib/language-labels";
import { readPasteSource } from "@/lib/paste-source";
import * as m from "@/paraglide/messages";

import { QuickSubmitButton } from "./quick-submit-button";
import type { SubmitStatus } from "./use-submit-transition";

// Adding a language touches four places, and missing one fails silently:
//   1. LANGUAGES here: the select option, and editor highlighting if `load` is given
//   2. lib/detect-language.ts: detection on paste (its id must match the value here)
//   3. bundledLanguages in lib/shiki.ts: highlighting on the share page, which
//      otherwise falls back to plain text
//   4. LANGUAGE_ALIASES in lib/paste-source.ts: recognising the language when
//      pasting from VS Code or a file
// Mermaid is the exception: the share page renders it instead of highlighting
// it, so it is not in shiki. SVG is highlighted by shiki as xml
// (HIGHLIGHT_ALIASES in text-preview.tsx).
//
// Grammars are loaded on demand when selected: imported statically, all 13
// would land in the home page chunk, and most people use one or two.
const LANGUAGES: readonly {
  label: string;
  value: string;
  load?: () => Promise<Extension>;
}[] = [
  { label: LANGUAGE_LABELS.plaintext, value: "plaintext" },
  {
    label: LANGUAGE_LABELS.javascript,
    load: async () => {
      const { javascript } = await import("@codemirror/lang-javascript");
      return javascript({ jsx: true });
    },
    value: "javascript",
  },
  {
    label: LANGUAGE_LABELS.typescript,
    load: async () => {
      const { javascript } = await import("@codemirror/lang-javascript");
      return javascript({
        jsx: true,
        typescript: true,
      });
    },
    value: "typescript",
  },
  {
    label: LANGUAGE_LABELS.python,
    load: async () => {
      const { python } = await import("@codemirror/lang-python");
      return python();
    },
    value: "python",
  },
  {
    label: LANGUAGE_LABELS.html,
    load: async () => {
      const { html } = await import("@codemirror/lang-html");
      return html();
    },
    value: "html",
  },
  {
    label: LANGUAGE_LABELS.css,
    load: async () => {
      const { css } = await import("@codemirror/lang-css");
      return css();
    },
    value: "css",
  },
  {
    label: LANGUAGE_LABELS.json,
    load: async () => {
      const { json } = await import("@codemirror/lang-json");
      return json();
    },
    value: "json",
  },
  {
    label: LANGUAGE_LABELS.markdown,
    load: async () => {
      const { markdown } = await import("@codemirror/lang-markdown");
      return markdown();
    },
    value: "markdown",
  },
  {
    label: LANGUAGE_LABELS.sql,
    load: async () => {
      const { sql } = await import("@codemirror/lang-sql");
      return sql();
    },
    value: "sql",
  },
  {
    label: LANGUAGE_LABELS.java,
    load: async () => {
      const { java } = await import("@codemirror/lang-java");
      return java();
    },
    value: "java",
  },
  {
    label: LANGUAGE_LABELS.cpp,
    load: async () => {
      const { cpp } = await import("@codemirror/lang-cpp");
      return cpp();
    },
    value: "cpp",
  },
  {
    label: LANGUAGE_LABELS.rust,
    load: async () => {
      const { rust } = await import("@codemirror/lang-rust");
      return rust();
    },
    value: "rust",
  },
  {
    label: LANGUAGE_LABELS.go,
    load: async () => {
      const { go } = await import("@codemirror/lang-go");
      return go();
    },
    value: "go",
  },
  {
    label: LANGUAGE_LABELS.yaml,
    load: async () => {
      const { yaml } = await import("@codemirror/lang-yaml");
      return yaml();
    },
    value: "yaml",
  },
  {
    label: LANGUAGE_LABELS.php,
    load: async () => {
      const { php } = await import("@codemirror/lang-php");
      return php();
    },
    value: "php",
  },
  // Shell highlighting would need @codemirror/legacy-modes.
  { label: LANGUAGE_LABELS.shellscript, value: "shellscript" },
  // Mermaid has no CodeMirror grammar, so the editor stays plain text; the
  // share page renders it as a diagram instead of highlighting it with shiki.
  { label: LANGUAGE_LABELS.mermaid, value: "mermaid" },
  // @codemirror/lang-xml is not installed, so this borrows HTML's highlighting; the share
  // page uses shiki's xml.
  {
    label: LANGUAGE_LABELS.svg,
    load: async () => {
      const { html } = await import("@codemirror/lang-html");
      return html();
    },
    value: "svg",
  },
];

/** Languages that can be drawn get a preview beside the editor. */
const RENDERABLE = new Set(["mermaid", "svg"]);
const EXPANDED_EDITOR_HEIGHT = "65vh";
/**
 * A pasted file larger than this is not loaded into the editor: CodeMirror stalls on
 * large content.
 */
const MAX_PASTED_FILE_BYTES = 1_000_000;

interface CodeUploadFormProps {
  status: SubmitStatus;
  progress: number;
  onSubmit: (
    el: HTMLButtonElement | null,
    payload: { content: string; language: string; title?: string }
  ) => void;
}

export function CodeUploadForm({
  status,
  progress,
  onSubmit,
}: CodeUploadFormProps) {
  const buttonRef = useRef<HTMLButtonElement>(null);
  const dialogButtonRef = useRef<HTMLButtonElement>(null);
  const [language, setLanguage] = useState("plaintext");
  const [code, setCode] = useState("");
  const [expanded, setExpanded] = useState(false);
  // Once the user picks a language, guessing stops for good, so another paste does not
  // override it.
  const [wasGuessed, setWasGuessed] = useState(false);
  const [userPickedLanguage, setUserPickedLanguage] = useState(false);
  const [pane, setPane] = useState<"edit" | "preview">("edit");
  // paste fires before onChange: note the language the clipboard reports, for onChange to
  // use.
  const pastedLanguage = useRef<string | null>(null);
  // A pasted file's name, saved as the share title on submit; dropped when the content is
  // cleared.
  const [title, setTitle] = useState<string | null>(null);

  const handleLanguageChange = (value: string | null) => {
    setLanguage(value ?? "plaintext");
    setWasGuessed(false);
    setUserPickedLanguage(true);
  };

  const handleCodeChange = (value: string, viewUpdate: ViewUpdate) => {
    setCode(value);
    if (value.length === 0) {
      setTitle(null);
    }
    if (userPickedLanguage) {
      return;
    }
    // ExternalChange is an update the library pushed into the editor, not typing.
    const isExternal = viewUpdate.transactions.some((tr) =>
      tr.annotation(ExternalChange)
    );
    if (isExternal) {
      return;
    }
    // The clipboard's stated language beats a guess from content; used once, then typing
    // guesses as usual.
    const guess = pastedLanguage.current ?? detectLanguage(value);
    pastedLanguage.current = null;
    if (guess) {
      setLanguage(guess);
      setWasGuessed(true);
    }
  };

  // Capture phase: React's capture listener sits on the root, so it runs before
  // CodeMirror's paste handler on contentDOM, and so before onChange. Nothing is
  // intercepted; the text is inserted as usual.
  const handlePaste = (event: React.ClipboardEvent<HTMLDivElement>) => {
    const source = readPasteSource(event.clipboardData);
    if (source.kind !== "file") {
      pastedLanguage.current = source.language;
      return;
    }
    // CodeMirror inserts nothing for a file, so it is read here. Only when the
    // editor is empty, where replacing everything makes sense; inserting at the
    // cursor into existing content has no clear meaning, so it is skipped.
    const [file] = event.clipboardData.files;
    const isText = source.language !== null || file.type.startsWith("text/");
    if (code.length > 0 || !isText || file.size > MAX_PASTED_FILE_BYTES) {
      return;
    }
    event.preventDefault();
    (async () => {
      setCode(await file.text());
      setTitle(source.filename);
      // setCode counts as an ExternalChange and skips handleCodeChange's guess,
      // so the language is set here.
      if (!userPickedLanguage && source.language) {
        setLanguage(source.language);
        setWasGuessed(true);
      }
    })();
  };

  const mode = useLanguageMode(language);
  const extensions = mode ? [mode] : [];
  const isRenderable = RENDERABLE.has(language);

  const renderEditor = (height: string) => (
    <CodeEditor
      className="h-full"
      extensions={extensions}
      height={height}
      onPasteCapture={handlePaste}
      onChange={handleCodeChange}
      placeholder={m.upload_code_placeholder()}
      value={code}
    />
  );

  const renderPreview = () =>
    language === "svg" ? (
      // Always SvgImage (a blob in an <img>), never inline in the DOM: scripts in the SVG
      // would run.
      <SvgImage
        alt={m.code_tab_preview()}
        className="h-full min-h-0"
        source={code}
      />
    ) : (
      <MermaidDiagram
        className="h-full min-h-0"
        code={code}
        debounceMs={EDITOR_DEBOUNCE_MS}
        title={m.mermaid_diagram()}
      />
    );

  const handlePaneChange = (value: unknown) =>
    setPane(value === "preview" ? "preview" : "edit");

  const submitFrom = (origin: HTMLButtonElement | null) => {
    if (code.trim().length > 0) {
      onSubmit(origin, { content: code, language, title: title ?? undefined });
      setExpanded(false);
    }
  };

  const renderLanguageSelect = (id: string) => (
    <Select onValueChange={handleLanguageChange} value={language}>
      <SelectTrigger
        aria-describedby={wasGuessed ? `${id}-guessed` : undefined}
        id={id}
        size="sm"
      >
        {wasGuessed && (
          <Sparkles
            aria-hidden="true"
            className="size-3.5 text-muted-foreground"
          />
        )}
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {LANGUAGES.map((lang) => (
          <SelectItem key={lang.value} value={lang.value}>
            {lang.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <div className="flex items-baseline gap-2">
          <Label htmlFor="code-language">{m.common_language()}</Label>
          {wasGuessed && (
            <span
              className="text-muted-foreground text-xs"
              id="code-language-guessed"
            >
              {m.code_language_guessed()}
            </span>
          )}
        </div>
        <div className="flex items-center gap-1">
          {renderLanguageSelect("code-language")}
          <Button
            aria-label={m.code_editor_expand()}
            onClick={() => setExpanded(true)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <Maximize2 />
          </Button>
        </div>
      </div>
      {isRenderable ? (
        <Tabs onValueChange={handlePaneChange} value={pane}>
          <TabsList>
            <TabsTrigger value="edit">{m.code_tab_edit()}</TabsTrigger>
            <TabsTrigger value="preview">{m.code_tab_preview()}</TabsTrigger>
          </TabsList>
          <TabsContent className="min-w-0" keepMounted value="edit">
            {renderEditor("300px")}
          </TabsContent>
          {/* flex-none: same reason as the dialog's preview tab below. */}
          <TabsContent
            className="h-[300px] min-w-0 flex-none"
            keepMounted
            value="preview"
          >
            {renderPreview()}
          </TabsContent>
        </Tabs>
      ) : (
        renderEditor("300px")
      )}
      <div className="flex justify-end">
        <QuickSubmitButton
          label={m.upload_send_snippet()}
          onClick={() => submitFrom(buttonRef.current)}
          progress={progress}
          ref={buttonRef}
          status={status}
        />
      </div>

      <Dialog onOpenChange={setExpanded} open={expanded}>
        {/* Wider side by side, or the editor and the diagram are both too narrow. */}
        <DialogContent
          className={
            isRenderable
              ? "w-[95vw] max-w-6xl sm:max-w-6xl"
              : "w-[92vw] max-w-4xl sm:max-w-4xl"
          }
        >
          <DialogHeader>
            <DialogTitle>{m.upload_tab_code()}</DialogTitle>
          </DialogHeader>
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-baseline gap-2">
              <Label htmlFor="code-language-expanded">
                {m.common_language()}
              </Label>
              {wasGuessed && (
                <span
                  className="text-muted-foreground text-xs"
                  id="code-language-expanded-guessed"
                >
                  {m.code_language_guessed()}
                </span>
              )}
            </div>
            {renderLanguageSelect("code-language-expanded")}
          </div>
          {isRenderable ? (
            <>
              {/* Wide screens: editor and diagram side by side. */}
              <div className="hidden min-w-0 gap-3 sm:grid sm:grid-cols-2">
                {renderEditor(EXPANDED_EDITOR_HEIGHT)}
                <div className="h-[65vh] min-w-0">{renderPreview()}</div>
              </div>
              {/* Narrow screens cannot fit two columns, so fall back to tabs. */}
              <Tabs
                className="min-w-0 sm:hidden"
                onValueChange={handlePaneChange}
                value={pane}
              >
                <TabsList>
                  <TabsTrigger value="edit">{m.code_tab_edit()}</TabsTrigger>
                  <TabsTrigger value="preview">
                    {m.code_tab_preview()}
                  </TabsTrigger>
                </TabsList>
                <TabsContent className="min-w-0" keepMounted value="edit">
                  {renderEditor(EXPANDED_EDITOR_HEIGHT)}
                </TabsContent>
                {/* flex-none: TabsContent defaults to flex-1 (basis 0), which
                    overrides h-[65vh]; the preview inside is absolute and adds
                    no height, so the box would collapse to 0. */}
                <TabsContent
                  className="h-[65vh] min-w-0 flex-none"
                  keepMounted
                  value="preview"
                >
                  {renderPreview()}
                </TabsContent>
              </Tabs>
            </>
          ) : (
            renderEditor(EXPANDED_EDITOR_HEIGHT)
          )}
          <div className="flex justify-end">
            <QuickSubmitButton
              label={m.upload_send_snippet()}
              onClick={() => submitFrom(dialogButtonRef.current)}
              progress={progress}
              ref={dialogButtonRef}
              status={status}
            />
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/**
 * Loaded grammars live at module level: the form unmounts on a tab switch, and coming
 * back should not flash plain text first.
 */
const loadedModes = new Map<string, Extension>();

/**
 * The CodeMirror grammar for the current language; undefined (plain text) while loading,
 * on failure, or when there is none.
 */
function useLanguageMode(language: string): Extension | undefined {
  // Results are kept in loadedModes; this only triggers one re-render when a load
  // finishes.
  const [, notifyLoaded] = useReducer((count: number) => count + 1, 0);

  useEffect(() => {
    const load = LANGUAGES.find((lang) => lang.value === language)?.load;
    if (!load || loadedModes.has(language)) {
      return;
    }

    let cancelled = false;
    (async () => {
      try {
        loadedModes.set(language, await load());
      } catch {
        // A grammar that fails to load (offline, an old chunk gone after a deploy) leaves
        // plain text; highlighting is an aid and must not block submitting.
        return;
      }
      if (!cancelled) {
        notifyLoaded();
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [language]);

  return loadedModes.get(language);
}
