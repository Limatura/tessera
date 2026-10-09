import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { Skeleton } from "@tessera/ui/components/skeleton";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@tessera/ui/components/tabs";
import { AnimatePresence, motion } from "motion/react";
import { Suspense, lazy, useRef } from "react";

import { GlassBackground } from "@/components/glass-background";
import type { FileEntry } from "@/lib/upload";
import { upload } from "@/lib/upload";
import * as m from "@/paraglide/messages";

import { BlobBackground } from "./blob-background";
import { LinkUploadForm } from "./link-upload-form";
import { RecentShares } from "./recent-shares";
import { UploadButton } from "./upload-button";
import { UploadResult } from "./upload-result";
import { useSubmitTransition } from "./use-submit-transition";

// 程式碼分頁帶著 CodeMirror(~200KB gzip),但首頁預設停在「檔案」,大部分人
// 不會點過去,所以拆成獨立 chunk,滑到或 focus 分頁標籤時才抓,點下去通常
// 已經抓完了。
const loadCodeUploadForm = () => import("./code-upload-form");

const CodeUploadForm = lazy(async () => {
  const { CodeUploadForm: component } = await loadCodeUploadForm();
  return { default: component };
});

export function UploadPanel() {
  const { status, progress, origin, expanded, result, start, reset } =
    useSubmitTransition();
  const fileButtonRef = useRef<HTMLButtonElement>(null);

  const handleSelectFiles = (files: FileEntry[]) => {
    start(fileButtonRef.current, (onProgress) =>
      upload({ files, type: "file" }, { onProgress })
    );
  };

  const handleSubmitCode = (
    el: HTMLButtonElement | null,
    payload: { content: string; language: string }
  ) => {
    start(el, (onProgress) =>
      upload(
        {
          content: payload.content,
          language: payload.language,
          type: "paste",
        },
        { onProgress }
      )
    );
  };

  const handleSubmitLink = (
    el: HTMLButtonElement | null,
    payload: { url: string }
  ) => {
    start(el, (onProgress) =>
      upload({ type: "link", url: payload.url }, { onProgress })
    );
  };

  return (
    <>
      <GlassBackground />
      <div className="m-auto w-full max-w-sm">
        <Card className="border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
          <CardHeader>
            <CardTitle className="font-medium text-xl tracking-tight">
              {m.upload_share_something()}
            </CardTitle>
            <CardDescription>{m.upload_share_description()}</CardDescription>
          </CardHeader>
          <CardContent>
            <Tabs defaultValue="file">
              <TabsList className="w-full">
                <TabsTrigger className="flex-1" value="file">
                  {m.upload_tab_file()}
                </TabsTrigger>
                <TabsTrigger
                  className="flex-1"
                  onFocus={loadCodeUploadForm}
                  onPointerEnter={loadCodeUploadForm}
                  value="code"
                >
                  {m.upload_tab_code()}
                </TabsTrigger>
                <TabsTrigger className="flex-1" value="link">
                  {m.upload_tab_link()}
                </TabsTrigger>
              </TabsList>

              <TabsContent value="file">
                <div className="flex justify-center py-4">
                  <UploadButton
                    onSelectFiles={handleSelectFiles}
                    progress={progress}
                    ref={fileButtonRef}
                    status={status}
                  />
                </div>
              </TabsContent>
              <TabsContent className="py-2" value="code">
                <Suspense fallback={<CodeUploadFormSkeleton />}>
                  <CodeUploadForm
                    onSubmit={handleSubmitCode}
                    progress={progress}
                    status={status}
                  />
                </Suspense>
              </TabsContent>
              <TabsContent className="py-2" value="link">
                <LinkUploadForm
                  onSubmit={handleSubmitLink}
                  progress={progress}
                  status={status}
                />
              </TabsContent>
            </Tabs>
          </CardContent>
        </Card>
        <RecentShares />
      </div>

      <AnimatePresence>
        {origin && (
          <motion.div
            animate={{
              clipPath: expanded
                ? `circle(${origin.radius}px at ${origin.x}px ${origin.y}px)`
                : `circle(0px at ${origin.x}px ${origin.y}px)`,
            }}
            className="fixed inset-0 z-50 flex items-center justify-center bg-melt text-melt-foreground"
            // 先讓瀏覽器把這層提升成獨立圖層,不等動畫開始才提升(提升本身會掉一幀)
            style={{ willChange: "clip-path" }}
            exit={{
              clipPath: `circle(0px at ${origin.x}px ${origin.y}px)`,
            }}
            initial={{
              clipPath: `circle(0px at ${origin.x}px ${origin.y}px)`,
            }}
            transition={{ duration: 0.7, ease: [0.65, 0, 0.35, 1] }}
          >
            <BlobBackground
              className="pointer-events-none absolute inset-0 overflow-hidden"
              vignette={false}
            />
            {result && (
              <UploadResult
                onReset={reset}
                result={result}
                visible={expanded}
              />
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </>
  );
}

/** 跟 CodeUploadForm 同樣骨架高度,載完換上去時卡片不會跳。 */
function CodeUploadFormSkeleton() {
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <Skeleton className="h-4 w-16" />
        <Skeleton className="h-7 w-32" />
      </div>
      <Skeleton className="h-[300px] w-full" />
      <div className="flex justify-end">
        <Skeleton className="h-9 w-32" />
      </div>
    </div>
  );
}
