import { useTheme } from "@tessera/ui/components/theme-provider";
import { cn } from "@tessera/ui/lib/utils";
import { vscodeDark, vscodeLight } from "@uiw/codemirror-theme-vscode";
import CodeMirror, { EditorView } from "@uiw/react-codemirror";
import type {
  Extension,
  ReactCodeMirrorProps,
  ReactCodeMirrorRef,
} from "@uiw/react-codemirror";
import { useMemo } from "react";
import type { Ref } from "react";

// className 只落在最外層 .cm-theme,但真正被長行撐開的是內層的 .cm-editor
// (它自己沒有 min-width:0,會被 .cm-content 由內往外推寬),所以外層歸零不夠,
// 要一路把 .cm-editor 也釘在容器寬度,.cm-scroller 才會接管水平捲動。
const EDITOR_CLIP =
  "min-w-0 overflow-hidden [&_.cm-editor]:w-full [&_.cm-editor]:min-w-0 [&_.cm-scroller]:min-w-0";

const NO_EXTENSIONS: Extension[] = [];

interface CodeEditorProps extends Omit<ReactCodeMirrorProps, "theme"> {
  /**
   * 指向可見標籤的 id。CodeMirror 真正能打字的是內層的 `.cm-content`,
   * `<label htmlFor>` 連不到它,只能把 aria 屬性直接掛上去。
   */
  labelledBy?: string;
  /** 要直接操作編輯器(例如在游標處插入文字)時用,`.view` 是 CodeMirror 的 EditorView。 */
  ref?: Ref<ReactCodeMirrorRef>;
}

/**
 * 全站共用的程式碼編輯器:主題跟著明暗模式走,長行不會把外框撐破。
 *
 * 語法、補全這類跟內容有關的東西由呼叫端用 `extensions` 帶進來。
 */
export function CodeEditor({
  className,
  extensions = NO_EXTENSIONS,
  labelledBy,
  ref,
  ...props
}: CodeEditorProps) {
  const { resolvedTheme } = useTheme();

  const withLabel = useMemo(
    () =>
      labelledBy
        ? [
            ...extensions,
            EditorView.contentAttributes.of({
              "aria-labelledby": labelledBy,
            }),
          ]
        : extensions,
    [extensions, labelledBy]
  );

  return (
    <CodeMirror
      className={cn(
        EDITOR_CLIP,
        "rounded-md border font-mono text-sm",
        className
      )}
      extensions={withLabel}
      ref={ref}
      theme={resolvedTheme === "dark" ? vscodeDark : vscodeLight}
      {...props}
    />
  );
}
