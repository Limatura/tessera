import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@tessera/ui/components/tabs";
import { AnimatePresence, motion } from "motion/react";
import { useRef } from "react";

import { GlassBackground } from "@/components/glass-background";
import type { FileEntry } from "@/lib/upload";
import { upload } from "@/lib/upload";
import * as m from "@/paraglide/messages";

import { BlobBackground } from "./blob-background";
import { CodeUploadForm } from "./code-upload-form";
import { LinkUploadForm } from "./link-upload-form";
import { UploadButton } from "./upload-button";
import { UploadResult } from "./upload-result";
import { useSubmitTransition } from "./use-submit-transition";

export function UploadPanel() {
  const { status, progress, origin, expanded, result, start, reset } =
    useSubmitTransition();
  const fileButtonRef = useRef<HTMLButtonElement>(null);

  const handleSelectFiles = (files: FileEntry[]) => {
    // biome-ignore lint: temp debug
    console.log("[upload-panel] handleSelectFiles", {
      count: files.length,
      hasButtonRef: !!fileButtonRef.current,
    });
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
      <div className="w-full max-w-sm">
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
                <TabsTrigger className="flex-1" value="code">
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
                <CodeUploadForm
                  onSubmit={handleSubmitCode}
                  progress={progress}
                  status={status}
                />
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
      </div>

      <AnimatePresence>
        {origin && (
          <motion.div
            animate={{
              clipPath: expanded
                ? `circle(${origin.radius}px at ${origin.x}px ${origin.y}px)`
                : `circle(0px at ${origin.x}px ${origin.y}px)`,
            }}
            className="fixed inset-0 z-50 flex items-center justify-center bg-[color-mix(in_oklch,var(--primary)_45%,white_55%)]"
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
            {expanded && result && (
              <UploadResult onReset={reset} result={result} />
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </>
  );
}
