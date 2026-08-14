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
import { Label } from "@tessera/ui/components/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { vscodeDark, vscodeLight } from "@uiw/codemirror-theme-vscode";
import CodeMirror from "@uiw/react-codemirror";
import type { Extension } from "@uiw/react-codemirror";
import { useEffect, useRef, useState } from "react";

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
};

interface CodeUploadFormProps {
  status: SubmitStatus;
  progress: number;
  onSubmit: (el: HTMLButtonElement | null) => void;
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
  const [language, setLanguage] = useState("plaintext");
  const [code, setCode] = useState("");
  const isDark = useIsDarkMode();

  const handleLanguageChange = (value: string | null) => {
    setLanguage(value ?? "plaintext");
  };

  const extensions = LANGUAGE_EXTENSIONS[language]
    ? [LANGUAGE_EXTENSIONS[language]]
    : [];

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <Label htmlFor="code-language">Language</Label>
        <Select value={language} onValueChange={handleLanguageChange}>
          <SelectTrigger id="code-language" size="sm">
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
      </div>
      <CodeMirror
        value={code}
        onChange={setCode}
        extensions={extensions}
        theme={isDark ? vscodeDark : vscodeLight}
        placeholder="paste your snippet..."
        height="200px"
        className="font-mono text-sm rounded-md border"
      />
      <div className="flex justify-end">
        <QuickSubmitButton
          label="Send snippet"
          onClick={() => onSubmit(buttonRef.current)}
          progress={progress}
          ref={buttonRef}
          status={status}
        />
      </div>
    </div>
  );
}
