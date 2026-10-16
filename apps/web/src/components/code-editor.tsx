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

// className lands only on the outer .cm-theme, but long lines widen the inner
// .cm-editor (it has no min-width: 0 and .cm-content pushes it wider). Zeroing
// the outer one is not enough: .cm-editor must also be pinned to the container
// width so .cm-scroller takes over horizontal scrolling.
const EDITOR_CLIP =
  "min-w-0 overflow-hidden [&_.cm-editor]:w-full [&_.cm-editor]:min-w-0 [&_.cm-scroller]:min-w-0";

const NO_EXTENSIONS: Extension[] = [];

interface CodeEditorProps extends Omit<ReactCodeMirrorProps, "theme"> {
  /**
   * Id of the visible label. CodeMirror's editable element is the inner
   * `.cm-content`, which `<label htmlFor>` cannot reach, so the aria attribute
   * is set directly.
   */
  labelledBy?: string;
  /**
   * For driving the editor directly (inserting text at the cursor, say);
   * `.view` is CodeMirror's EditorView.
   */
  ref?: Ref<ReactCodeMirrorRef>;
}

/**
 * The shared code editor: its theme follows light/dark mode and long lines do
 * not break the frame. Content-specific behaviour such as syntax and completion
 * comes from the caller through `extensions`.
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
