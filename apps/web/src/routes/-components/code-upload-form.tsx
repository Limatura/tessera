import { cpp } from "@codemirror/lang-cpp";
import { css } from "@codemirror/lang-css";
import { go } from "@codemirror/lang-go";
import { html } from "@codemirror/lang-html";
import { java } from "@codemirror/lang-java";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { php } from "@codemirror/lang-php";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";
import { sql } from "@codemirror/lang-sql";
import { yaml } from "@codemirror/lang-yaml";
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
import { vscodeDark, vscodeLight } from "@uiw/codemirror-theme-vscode";
import CodeMirror, { ExternalChange } from "@uiw/react-codemirror";
import type { Extension, ViewUpdate } from "@uiw/react-codemirror";
import { Maximize2, Sparkles } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { detectLanguage } from "@/lib/detect-language";
import * as m from "@/paraglide/messages";

import { QuickSubmitButton } from "./quick-submit-button";
import type { SubmitStatus } from "./use-submit-transition";

const LANGUAGES = [
  { label: "Plain text", value: "plaintext" },
  { label: "JavaScript", value: "javascript" },
  { label: "TypeScript", value: "typescript" },
  { label: "Python", value: "python" },
  { label: "HTML", value: "html" },
  { label: "CSS", value: "css" },
  { label: "JSON", value: "json" },
  { label: "Markdown", value: "markdown" },
  { label: "SQL", value: "sql" },
  { label: "Java", value: "java" },
  { label: "C++", value: "cpp" },
  { label: "Rust", value: "rust" },
  { label: "Go", value: "go" },
  { label: "YAML", value: "yaml" },
  { label: "PHP", value: "php" },
  { label: "Shell", value: "shellscript" },
] as const;

const LANGUAGE_EXTENSIONS: Record<string, Extension> = {
  cpp: cpp(),
  css: css(),
  go: go(),
  html: html(),
  java: java(),
  javascript: javascript({ jsx: true }),
  json: json(),
  markdown: markdown(),
  php: php(),
  python: python(),
  rust: rust(),
  sql: sql(),
  typescript: javascript({ jsx: true, typescript: true }),
  yaml: yaml(),
};

interface CodeUploadFormProps {
  status: SubmitStatus;
  progress: number;
  onSubmit: (
    el: HTMLButtonElement | null,
    payload: { content: string; language: string }
  ) => void;
}

function useIsDarkMode() {
  const [isDark, setIsDark] = useState(
    () =>
      typeof document !== "undefined" &&
      document.documentElement.classList.contains("dark")
  );

  useEffect(() => {
    const root = document.documentElement;
    const observer = new MutationObserver(() => {
      setIsDark(root.classList.contains("dark"));
    });
    observer.observe(root, { attributeFilter: ["class"], attributes: true });
    return () => observer.disconnect();
  }, []);

  return isDark;
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
  // 目前的語言是猜的還是使用者選的。
  // 使用者自己選過之後就永久停止猜測,免得之後再貼一次又被蓋掉(tessera#228)。
  const [wasGuessed, setWasGuessed] = useState(false);
  const [userPickedLanguage, setUserPickedLanguage] = useState(false);
  const isDark = useIsDarkMode();

  const handleLanguageChange = (value: string | null) => {
    setLanguage(value ?? "plaintext");
    setWasGuessed(false);
    setUserPickedLanguage(true);
  };

  const handleCodeChange = (value: string, viewUpdate: ViewUpdate) => {
    setCode(value);
    if (userPickedLanguage) {
      return;
    }
    // 套件對「從 value prop 推回編輯器」的更新會標上 ExternalChange,
    // 跳過這種,只對使用者真正的編輯做猜測。
    const isExternal = viewUpdate.transactions.some((tr) =>
      tr.annotation(ExternalChange)
    );
    if (isExternal) {
      return;
    }
    const guess = detectLanguage(value);
    if (guess) {
      setLanguage(guess);
      setWasGuessed(true);
    }
  };

  const extensions = LANGUAGE_EXTENSIONS[language]
    ? [LANGUAGE_EXTENSIONS[language]]
    : [];

  const submitFrom = (origin: HTMLButtonElement | null) => {
    if (code.trim().length > 0) {
      onSubmit(origin, { content: code, language });
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
          <Label htmlFor="code-language">Language</Label>
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
      <CodeMirror
        className="rounded-md border font-mono text-sm"
        extensions={extensions}
        height="300px"
        onChange={handleCodeChange}
        placeholder="paste your snippet..."
        theme={isDark ? vscodeDark : vscodeLight}
        value={code}
      />
      <div className="flex justify-end">
        <QuickSubmitButton
          label="Send snippet"
          onClick={() => submitFrom(buttonRef.current)}
          progress={progress}
          ref={buttonRef}
          status={status}
        />
      </div>

      <Dialog onOpenChange={setExpanded} open={expanded}>
        <DialogContent className="w-[92vw] max-w-4xl sm:max-w-4xl">
          <DialogHeader>
            <DialogTitle>{m.upload_tab_code()}</DialogTitle>
          </DialogHeader>
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-baseline gap-2">
              <Label htmlFor="code-language-expanded">Language</Label>
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
          <CodeMirror
            className="rounded-md border font-mono text-sm"
            extensions={extensions}
            height="65vh"
            onChange={handleCodeChange}
            placeholder="paste your snippet..."
            theme={isDark ? vscodeDark : vscodeLight}
            value={code}
          />
          <div className="flex justify-end">
            <QuickSubmitButton
              label="Send snippet"
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
