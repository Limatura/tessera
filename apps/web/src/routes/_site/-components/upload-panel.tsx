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
import type { FileEntry, UploadResultData } from "@tessera/upload";
import { AnimatePresence, motion } from "motion/react";
import { Suspense, lazy, useRef, useState } from "react";

import { GlassBackground } from "@/components/glass-background";
import { useCapabilities } from "@/hooks/use-capabilities";
import type { SharedDraft } from "@/lib/pwa/shared-payload";
import { uploader } from "@/lib/uploader";
import * as m from "@/paraglide/messages";

import { BlobBackground } from "./blob-background";
import type { EncryptionState } from "./encryption-field";
import {
  EncryptionField,
  NO_ENCRYPTION,
  encryptionChoice,
} from "./encryption-field";
import { LinkUploadForm } from "./link-upload-form";
import {
  MetadataField,
  offersMetadata,
  stripsMetadata,
} from "./metadata-field";
import { RecentShares } from "./recent-shares";
import { SharedFiles } from "./shared-files";
import { UploadButton } from "./upload-button";
import { UploadResult } from "./upload-result";
import {
  COLLAPSE_TRANSITION,
  EXPAND_TRANSITION,
  useSubmitTransition,
} from "./use-submit-transition";
import type { Origin } from "./use-submit-transition";

// The code tab carries CodeMirror (~200KB gzip), but the home page opens on
// "files" and most people never switch, so it is a separate chunk fetched on
// hover or focus of the tab label, usually done before the click.
const loadCodeUploadForm = () => import("./code-upload-form");

const CodeUploadForm = lazy(async () => {
  const { CodeUploadForm: component } = await loadCodeUploadForm();
  return { default: component };
});

interface UploadPanelProps {
  /** What the system share sheet sent, to prefill the matching tab with. */
  shared?: SharedDraft | null;
  /** The shared files were uploaded or discarded. */
  onSharedDone?: () => void;
}

export function UploadPanel({ shared, onSharedDone }: UploadPanelProps) {
  const {
    collapsed,
    status,
    progress,
    origin,
    expanded,
    result,
    start,
    reset,
  } = useSubmitTransition();
  const fileButtonRef = useRef<HTMLButtonElement>(null);
  const capabilities = useCapabilities();
  const [tab, setTab] = useState("file");
  const [encryption, setEncryption] = useState<EncryptionState>(NO_ENCRYPTION);
  const [showEncryptionError, setShowEncryptionError] = useState(false);
  const [keepMetadata, setKeepMetadata] = useState(false);
  // Remounts the link and code forms on each share, so their prefill applies
  // even when the previous share went to the same tab.
  const [sharedKey, setSharedKey] = useState(0);
  // Adjusted during render, not in an effect: the tab must already be right on
  // the render that shows the prefilled form.
  const [seenShared, setSeenShared] = useState(shared);
  if (shared !== seenShared) {
    setSeenShared(shared);
    if (shared) {
      setTab(shared.kind);
      setSharedKey((key) => key + 1);
    }
  }
  // A link share is a URL the server must read to redirect, so it is never sealed
  const offerEncryption =
    capabilities.data?.encryption === true && tab !== "link";
  const offerMetadata = offersMetadata(capabilities.data, tab);

  /**
   * The sealing to upload with, or `null` (with the error shown) when the
   * password is too short and nothing should be sent.
   */
  const sealing = () => {
    if (!offerEncryption) {
      return;
    }
    const choice = encryptionChoice(encryption);
    if (choice === null) {
      setShowEncryptionError(true);
    }
    return choice;
  };

  const handleSelectFiles = (
    files: FileEntry[],
    el: HTMLElement | null = fileButtonRef.current
  ) => {
    const choice = sealing();
    if (choice === null) {
      return;
    }
    start(el, (onProgress) =>
      uploader.upload(
        { files, type: "file" },
        {
          encryption: choice,
          onProgress,
          stripMetadata: stripsMetadata(offerMetadata, keepMetadata),
        }
      )
    );
  };

  const handleSubmitCode = (
    el: HTMLButtonElement | null,
    payload: { content: string; language: string; title?: string }
  ) => {
    const choice = sealing();
    if (choice === null) {
      return;
    }
    start(el, (onProgress) =>
      uploader.upload(
        {
          content: payload.content,
          language: payload.language,
          title: payload.title,
          type: "paste",
        },
        { encryption: choice, onProgress }
      )
    );
  };

  const handleSubmitLink = (
    el: HTMLButtonElement | null,
    payload: { url: string }
  ) => {
    start(el, (onProgress) =>
      uploader.upload({ type: "link", url: payload.url }, { onProgress })
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
            <Tabs onValueChange={(value) => setTab(String(value))} value={tab}>
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
                {shared?.kind === "file" ? (
                  <SharedFiles
                    files={shared.files}
                    onDiscard={() => onSharedDone?.()}
                    onUpload={(el) =>
                      handleSelectFiles(
                        shared.files.map((file) => ({ file, path: file.name })),
                        el
                      )
                    }
                    progress={progress}
                    status={status}
                  />
                ) : (
                  <div className="flex justify-center py-4">
                    <UploadButton
                      onSelectFiles={(files) => handleSelectFiles(files)}
                      progress={progress}
                      ref={fileButtonRef}
                      status={status}
                    />
                  </div>
                )}
              </TabsContent>
              <TabsContent className="py-2" value="code">
                <Suspense fallback={<CodeUploadFormSkeleton />}>
                  <CodeUploadForm
                    initialContent={
                      shared?.kind === "code" ? shared.content : undefined
                    }
                    initialTitle={
                      shared?.kind === "code" ? shared.title : undefined
                    }
                    key={sharedKey}
                    onSubmit={handleSubmitCode}
                    progress={progress}
                    status={status}
                  />
                </Suspense>
              </TabsContent>
              <TabsContent className="py-2" value="link">
                <LinkUploadForm
                  initialUrl={shared?.kind === "link" ? shared.url : undefined}
                  key={sharedKey}
                  onSubmit={handleSubmitLink}
                  progress={progress}
                  status={status}
                />
              </TabsContent>
            </Tabs>
            {offerMetadata && (
              <MetadataField keep={keepMetadata} onChange={setKeepMetadata} />
            )}
            {offerEncryption && (
              <EncryptionField
                onChange={(next) => {
                  setEncryption(next);
                  setShowEncryptionError(false);
                }}
                showError={showEncryptionError}
                state={encryption}
              />
            )}
          </CardContent>
        </Card>
        <RecentShares />
      </div>

      <ResultOverlay
        collapsed={collapsed}
        expanded={expanded}
        onReset={() => {
          if (shared?.kind === "file") {
            onSharedDone?.();
          }
          reset();
        }}
        origin={origin}
        result={result}
      />
    </>
  );
}

/** Same height as CodeUploadForm, so the card does not jump when it loads. */
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

interface ResultOverlayProps {
  origin: Origin | null;
  expanded: boolean;
  result: UploadResultData | null;
  collapsed: () => void;
  onReset: () => void;
}

/** The circular reveal that covers the page with the upload result. */
function ResultOverlay({
  origin,
  expanded,
  result,
  collapsed,
  onReset,
}: ResultOverlayProps) {
  return (
    <AnimatePresence>
      {origin && (
        // The overlay is two layers so the expensive part never repaints:
        // the colour and its animated canvas sit on a circle grown with
        // `transform: scale`, which the compositor animates without
        // repainting, unlike an animated clip-path over a full-screen,
        // constantly redrawing canvas (that dropped frames on phones). Only
        // the static result content keeps a clip-path, so it still appears
        // under the circle's edge rather than over the page.
        <div className="pointer-events-none fixed inset-0 z-50">
          <motion.div
            animate={{ scale: expanded ? 1 : 0 }}
            className="absolute overflow-hidden rounded-full bg-melt"
            exit={{ scale: 0 }}
            initial={{ scale: 0 }}
            onAnimationComplete={collapsed}
            style={{
              height: origin.radius * 2,
              left: origin.x - origin.radius,
              top: origin.y - origin.radius,
              width: origin.radius * 2,
              willChange: "transform",
            }}
            transition={expanded ? EXPAND_TRANSITION : COLLAPSE_TRANSITION}
          >
            {/* Counter-offset so the canvas covers the viewport, not the circle's box */}
            <BlobBackground
              className="pointer-events-none absolute h-dvh w-dvw overflow-hidden"
              style={{
                left: origin.radius - origin.x,
                top: origin.radius - origin.y,
              }}
              vignette={false}
            />
          </motion.div>
          <motion.div
            animate={{
              clipPath: expanded
                ? `circle(${origin.radius}px at ${origin.x}px ${origin.y}px)`
                : `circle(0px at ${origin.x}px ${origin.y}px)`,
            }}
            className="pointer-events-auto absolute inset-0 flex items-center justify-center text-melt-foreground"
            exit={{
              clipPath: `circle(0px at ${origin.x}px ${origin.y}px)`,
            }}
            initial={{
              clipPath: `circle(0px at ${origin.x}px ${origin.y}px)`,
            }}
            transition={expanded ? EXPAND_TRANSITION : COLLAPSE_TRANSITION}
          >
            {result && (
              <UploadResult
                onReset={onReset}
                origin={origin}
                result={result}
                visible={expanded}
              />
            )}
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  );
}
