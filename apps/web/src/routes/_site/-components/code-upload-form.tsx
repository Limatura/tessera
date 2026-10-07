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
import { detectLanguage } from "@/lib/detect-language";
import * as m from "@/paraglide/messages";

import { QuickSubmitButton } from "./quick-submit-button";
import type { SubmitStatus } from "./use-submit-transition";

// 加一個語言要改三個地方,漏掉不會報錯,只會靜默少一塊功能:
//   1. 這裡的 LANGUAGES —— 選單選項,load 給了才有編輯器高亮
//   2. lib/detect-language.ts —— 貼上內容時的自動偵測(id 要跟這裡的 value 一致)
//   3. lib/shiki.ts 的 bundledLanguages —— 分享頁的語法高亮,
//      漏掉的話分享頁會靜默 fallback 成純文字
// Mermaid 是例外:它在分享頁走渲染而不是高亮,所以不進 shiki(見 tessera#227)。
//
// 語法包用 load 動態載入,選到才抓:13 包靜態 import 會全部落進首頁的 chunk,
// 而大部分人只會用到一兩種。
const LANGUAGES: readonly {
  label: string;
  value: string;
  load?: () => Promise<Extension>;
}[] = [
  { label: "Plain text", value: "plaintext" },
  {
    label: "JavaScript",
    load: async () => {
      const { javascript } = await import("@codemirror/lang-javascript");
      return javascript({ jsx: true });
    },
    value: "javascript",
  },
  {
    label: "TypeScript",
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
    label: "Python",
    load: async () => {
      const { python } = await import("@codemirror/lang-python");
      return python();
    },
    value: "python",
  },
  {
    label: "HTML",
    load: async () => {
      const { html } = await import("@codemirror/lang-html");
      return html();
    },
    value: "html",
  },
  {
    label: "CSS",
    load: async () => {
      const { css } = await import("@codemirror/lang-css");
      return css();
    },
    value: "css",
  },
  {
    label: "JSON",
    load: async () => {
      const { json } = await import("@codemirror/lang-json");
      return json();
    },
    value: "json",
  },
  {
    label: "Markdown",
    load: async () => {
      const { markdown } = await import("@codemirror/lang-markdown");
      return markdown();
    },
    value: "markdown",
  },
  {
    label: "SQL",
    load: async () => {
      const { sql } = await import("@codemirror/lang-sql");
      return sql();
    },
    value: "sql",
  },
  {
    label: "Java",
    load: async () => {
      const { java } = await import("@codemirror/lang-java");
      return java();
    },
    value: "java",
  },
  {
    label: "C++",
    load: async () => {
      const { cpp } = await import("@codemirror/lang-cpp");
      return cpp();
    },
    value: "cpp",
  },
  {
    label: "Rust",
    load: async () => {
      const { rust } = await import("@codemirror/lang-rust");
      return rust();
    },
    value: "rust",
  },
  {
    label: "Go",
    load: async () => {
      const { go } = await import("@codemirror/lang-go");
      return go();
    },
    value: "go",
  },
  {
    label: "YAML",
    load: async () => {
      const { yaml } = await import("@codemirror/lang-yaml");
      return yaml();
    },
    value: "yaml",
  },
  {
    label: "PHP",
    load: async () => {
      const { php } = await import("@codemirror/lang-php");
      return php();
    },
    value: "php",
  },
  // Shell 的高亮要另外裝 @codemirror/legacy-modes。
  { label: "Shell", value: "shellscript" },
  // Mermaid 沒有 CodeMirror 語法包,編輯器維持純文字;
  // 分享頁會直接把它渲染成圖,不走 shiki(tessera#227)。
  { label: "Mermaid", value: "mermaid" },
];

const MERMAID = "mermaid";
const EXPANDED_EDITOR_HEIGHT = "65vh";

interface CodeUploadFormProps {
  status: SubmitStatus;
  progress: number;
  onSubmit: (
    el: HTMLButtonElement | null,
    payload: { content: string; language: string }
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
  // 使用者自己選過就永久停止猜測,免得再貼一次又被蓋掉。
  const [wasGuessed, setWasGuessed] = useState(false);
  const [userPickedLanguage, setUserPickedLanguage] = useState(false);
  const [mermaidPane, setMermaidPane] = useState<"edit" | "preview">("edit");

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
    // ExternalChange 是套件推回編輯器的更新,不是使用者打的。
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

  const mode = useLanguageMode(language);
  const extensions = mode ? [mode] : [];
  const isMermaid = language === MERMAID;

  const renderEditor = (height: string) => (
    <CodeEditor
      className="h-full"
      extensions={extensions}
      height={height}
      onChange={handleCodeChange}
      placeholder={m.upload_code_placeholder()}
      value={code}
    />
  );

  const renderPreview = () => (
    <MermaidDiagram
      className="h-full min-h-0"
      code={code}
      debounceMs={EDITOR_DEBOUNCE_MS}
      title={m.mermaid_diagram()}
    />
  );

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
      <CodeEditor
        extensions={extensions}
        height="300px"
        onChange={handleCodeChange}
        placeholder={m.upload_code_placeholder()}
        value={code}
      />
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
        {/* 並排時要更寬,不然編輯器跟圖各自都太窄。 */}
        <DialogContent
          className={
            isMermaid
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
          {isMermaid ? (
            <>
              {/* 寬螢幕:編輯器跟圖並排,邊改邊看。 */}
              <div className="hidden min-w-0 gap-3 sm:grid sm:grid-cols-2">
                {renderEditor(EXPANDED_EDITOR_HEIGHT)}
                <div className="h-[65vh] min-w-0">{renderPreview()}</div>
              </div>
              {/* 窄螢幕塞不下兩欄,退回分頁。 */}
              <Tabs
                className="min-w-0 sm:hidden"
                onValueChange={(value) =>
                  setMermaidPane(value === "preview" ? "preview" : "edit")
                }
                value={mermaidPane}
              >
                <TabsList>
                  <TabsTrigger value="edit">{m.code_tab_edit()}</TabsTrigger>
                  <TabsTrigger value="preview">
                    {m.code_tab_preview()}
                  </TabsTrigger>
                </TabsList>
                <TabsContent className="min-w-0" value="edit">
                  {renderEditor(EXPANDED_EDITOR_HEIGHT)}
                </TabsContent>
                <TabsContent className="h-[65vh] min-w-0" value="preview">
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
 * 載過的語法包留在模組層:切分頁時這個表單會 unmount,回來時直接拿得到,
 * 不會先閃一下沒上色的純文字。
 */
const loadedModes = new Map<string, Extension>();

/** 目前語言的 CodeMirror 語法包;還在載、載失敗或這語言本來就沒有時回 undefined(=純文字)。 */
function useLanguageMode(language: string): Extension | undefined {
  // 結果存在 loadedModes,這裡只負責在載完時觸發一次重畫。
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
        // 抓不到語法包(離線、部署換版後舊 chunk 不在了)就留在純文字:
        // 高亮只是輔助,不該擋住貼上跟送出。
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
