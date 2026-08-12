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

import { BlobBackground } from "./blob-background";
import { CodeUploadForm } from "./code-upload-form";
import { LinkUploadForm } from "./link-upload-form";
import { TextUploadForm } from "./text-upload-form";
import { UploadButton } from "./upload-button";
import { UploadResult } from "./upload-result";
import { useSubmitTransition } from "./use-submit-transition";

export function UploadPanel() {
  const { status, progress, origin, expanded, start, reset } =
    useSubmitTransition();
  const fileButtonRef = useRef<HTMLButtonElement>(null);

  return (
    <div className="relative flex min-h-svh w-full items-center justify-center bg-background p-6">
      <BlobBackground />

      <div className="w-full max-w-sm">
        <Card className="border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
          <CardHeader>
            <CardTitle className="font-medium text-xl tracking-tight">
              Share something
            </CardTitle>
            <CardDescription>
              Upload a file, paste text or code, or shorten a link.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Tabs defaultValue="file">
              <TabsList className="w-full">
                <TabsTrigger className="flex-1" value="file">
                  File
                </TabsTrigger>
                <TabsTrigger className="flex-1" value="text">
                  Text
                </TabsTrigger>
                <TabsTrigger className="flex-1" value="code">
                  Code
                </TabsTrigger>
                <TabsTrigger className="flex-1" value="link">
                  Link
                </TabsTrigger>
              </TabsList>

              <TabsContent value="file">
                <div className="flex justify-center py-4">
                  <UploadButton
                    onClick={() => start(fileButtonRef.current)}
                    progress={progress}
                    ref={fileButtonRef}
                    status={status}
                  />
                </div>
              </TabsContent>
              <TabsContent className="py-2" value="text">
                <TextUploadForm
                  onSubmit={start}
                  progress={progress}
                  status={status}
                />
              </TabsContent>
              <TabsContent className="py-2" value="code">
                <CodeUploadForm
                  onSubmit={start}
                  progress={progress}
                  status={status}
                />
              </TabsContent>
              <TabsContent className="py-2" value="link">
                <LinkUploadForm
                  onSubmit={start}
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
            {expanded && <UploadResult onReset={reset} />}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
