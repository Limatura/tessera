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
import { LANGUAGE_LABELS } from "@/lib/language-labels";
import { readPasteSource } from "@/lib/paste-source";
import * as m from "@/paraglide/messages";

import { QuickSubmitButton } from "./quick-submit-button";
import type { SubmitStatus } from "./use-submit-transition";

// 加一個語言要改四個地方,漏掉不會報錯只會靜默少功能:
//   1. 這裡的 LANGUAGES —— 選單選項,load 給了才有編輯器高亮
//   2. lib/detect-language.ts —— 貼上自動偵測(id 要跟這裡的 value 一致)
//   3. lib/shiki.ts 的 bundledLanguages —— 分享頁語法高亮,漏掉會靜默 fallback 成純文字
//   4. lib/paste-source.ts 的 LANGUAGE_ALIASES —— 從 VS Code / 檔案貼上時直接認出語言
// Mermaid 例外:分享頁走渲染而非高亮,不進 shiki(見 tessera#227)。
//
// 語法包用 load 動態載入、選到才抓:13 包靜態 import 會全落進首頁 chunk,
// 大部分人只用一兩種。
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
  // Shell 的高亮要另外裝 @codemirror/legacy-modes。
  { label: LANGUAGE_LABELS.shellscript, value: "shellscript" },
  // Mermaid 沒有 CodeMirror 語法包,編輯器維持純文字;
  // 分享頁會直接把它渲染成圖,不走 shiki(tessera#227)。
  { label: LANGUAGE_LABELS.mermaid, value: "mermaid" },
];

const MERMAID = "mermaid";
const EXPANDED_EDITOR_HEIGHT = "65vh";
/** 貼上的檔案超過這個大小就不讀進編輯器:CodeMirror 塞太大的內容會卡。 */
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
  // 使用者選過就永久停止猜測,免得再貼一次又被蓋掉。
  const [wasGuessed, setWasGuessed] = useState(false);
  const [userPickedLanguage, setUserPickedLanguage] = useState(false);
  const [mermaidPane, setMermaidPane] = useState<"edit" | "preview">("edit");
  // paste 事件比 onChange 早觸發:先記下剪貼簿說的語言,onChange 再拿來用。
  const pastedLanguage = useRef<string | null>(null);
  // 貼上檔案時記下檔名,送出時存成分享標題;內容清空就跟著丟掉。
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
    // ExternalChange 是套件推回編輯器的更新,不是使用者打的。
    const isExternal = viewUpdate.transactions.some((tr) =>
      tr.annotation(ExternalChange)
    );
    if (isExternal) {
      return;
    }
    // 剪貼簿明講的語言比從內容猜的可靠;用一次就清掉,之後打字照常猜。
    const guess = pastedLanguage.current ?? detectLanguage(value);
    pastedLanguage.current = null;
    if (guess) {
      setLanguage(guess);
      setWasGuessed(true);
    }
  };

  // capture 階段:React 的 capture 監聽在 root 上,會比 CodeMirror 掛在
  // contentDOM 的 paste 先跑,也就比 onChange 早。不攔截,文字照常插入。
  const handlePaste = (event: React.ClipboardEvent<HTMLDivElement>) => {
    const source = readPasteSource(event.clipboardData);
    if (source.kind !== "file") {
      pastedLanguage.current = source.language;
      return;
    }
    // CodeMirror 收到檔案不會插入任何東西,要自己讀。只在編輯器是空的時候接手,
    // 整份換掉才合理;已經有內容時插到游標處的語意不清楚,乾脆不處理。
    const [file] = event.clipboardData.files;
    const isText = source.language !== null || file.type.startsWith("text/");
    if (code.length > 0 || !isText || file.size > MAX_PASTED_FILE_BYTES) {
      return;
    }
    event.preventDefault();
    (async () => {
      setCode(await file.text());
      setTitle(source.filename);
      // setCode 推回編輯器算 ExternalChange,不會走 handleCodeChange 的猜測,
      // 所以語言在這裡直接設。
      if (!userPickedLanguage && source.language) {
        setLanguage(source.language);
        setWasGuessed(true);
      }
    })();
  };

  const mode = useLanguageMode(language);
  const extensions = mode ? [mode] : [];
  const isMermaid = language === MERMAID;

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
      <CodeEditor
        extensions={extensions}
        height="300px"
        onPasteCapture={handlePaste}
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
                {/* flex-none:TabsContent 預設 flex-1(basis 0)會蓋過 h-[65vh],
                    預覽內層是 absolute 不撐高度,框會直接縮成 0。 */}
                <TabsContent
                  className="h-[65vh] min-w-0 flex-none"
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

/** 載過的語法包留在模組層:切分頁時表單會 unmount,回來時直接拿得到,不會先閃一下純文字。 */
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
        // 抓不到語法包(離線、換版後舊 chunk 消失)就留純文字——高亮只是輔助,不該擋貼上送出。
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
