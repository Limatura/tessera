import { cn } from "@tessera/ui/lib/utils";
import { Check, Upload } from "lucide-react";
import { motion } from "motion/react";
import { forwardRef, useRef, useState } from "react";

import type { FileEntry } from "@/lib/upload";
import * as m from "@/paraglide/messages";

import { BlobCanvas } from "./blob-canvas";
import { PRIMARY_CLUSTER_SPECS, PRIMARY_MELT_COLOR } from "./blob-specs";

interface UploadButtonProps {
  status: "idle" | "uploading" | "done";
  progress: number;
  onSelectFiles: (entries: FileEntry[]) => void;
}

function filesToEntries(files: FileList): FileEntry[] {
  const entries: FileEntry[] = [];
  for (const file of files) {
    const relativePath = (file as File & { webkitRelativePath?: string })
      .webkitRelativePath;
    entries.push({ file, path: relativePath || file.name });
  }
  return entries;
}

async function readAllEntries(
  reader: FileSystemDirectoryReader
): Promise<FileSystemEntry[]> {
  const all: FileSystemEntry[] = [];
  for (;;) {
    // oxlint-disable-next-line no-await-in-loop, promise/avoid-new -- 每次 readEntries 最多回 100 筆，只有 callback API，要照順序呼叫到回空陣列為止
    const batch = await new Promise<FileSystemEntry[]>((resolve, reject) => {
      reader.readEntries(resolve, reject);
    });
    if (batch.length === 0) {
      break;
    }
    all.push(...batch);
  }
  return all;
}

function readEntryFile(entry: FileSystemFileEntry): Promise<File> {
  // oxlint-disable-next-line promise/avoid-new -- FileSystemFileEntry.file only exposes a callback API
  return new Promise((resolve, reject) => {
    entry.file(resolve, reject);
  });
}

function stripLeadingSlash(path: string): string {
  return path.startsWith("/") ? path.slice(1) : path;
}

async function walkEntry(entry: FileSystemEntry): Promise<FileEntry[]> {
  if (entry.isFile) {
    const file = await readEntryFile(entry as FileSystemFileEntry);
    return [{ file, path: stripLeadingSlash(entry.fullPath) }];
  }
  if (entry.isDirectory) {
    const reader = (entry as FileSystemDirectoryEntry).createReader();
    const children = await readAllEntries(reader);
    const nested = await Promise.all(children.map(walkEntry));
    return nested.flat();
  }
  return [];
}

async function entriesFromDataTransfer(
  dataTransfer: DataTransfer
): Promise<FileEntry[]> {
  const items = [...dataTransfer.items];
  const fsEntries = items
    .map((item) => item.webkitGetAsEntry?.())
    .filter((entry): entry is FileSystemEntry => entry !== null);

  if (fsEntries.length === 0) {
    return filesToEntries(dataTransfer.files);
  }

  const nested = await Promise.all(fsEntries.map(walkEntry));
  return nested.flat();
}

export const UploadButton = forwardRef<HTMLButtonElement, UploadButtonProps>(
  ({ status, progress, onSelectFiles }, ref) => {
    const inputRef = useRef<HTMLInputElement>(null);
    const [isDragging, setIsDragging] = useState(false);

    const openPicker = (folderMode: boolean) => {
      // biome-ignore lint: temp debug
      console.log("[upload-button] openPicker", {
        folderMode,
        hasInput: !!inputRef.current,
        status,
      });
      if (status !== "idle" || !inputRef.current) {
        return;
      }
      inputRef.current.webkitdirectory = folderMode;
      inputRef.current.multiple = folderMode;
      // biome-ignore lint: temp debug
      console.log("[upload-button] calling input.click()", {
        multiple: inputRef.current.multiple,
        webkitdirectory: inputRef.current.webkitdirectory,
      });
      inputRef.current.click();
    };

    const handleClick = () => {
      // biome-ignore lint: temp debug
      console.log("[upload-button] main button clicked");
      openPicker(false);
    };

    const handleChooseFolder = () => {
      // biome-ignore lint: temp debug
      console.log("[upload-button] folder link clicked");
      openPicker(true);
    };

    const handleInputChange = (event: React.ChangeEvent<HTMLInputElement>) => {
      // biome-ignore lint: temp debug
      console.log(
        "[upload-button] handleInputChange fired",
        event.target.files?.length
      );
      const { files } = event.target;
      const entries = files ? filesToEntries(files) : [];
      event.target.value = "";
      if (entries.length > 0) {
        onSelectFiles(entries);
      }
    };

    const handleDragOver = (event: React.DragEvent<HTMLButtonElement>) => {
      event.preventDefault();
      if (status === "idle") {
        setIsDragging(true);
      }
    };

    const handleDragLeave = () => {
      setIsDragging(false);
    };

    const handleDrop = async (event: React.DragEvent<HTMLButtonElement>) => {
      event.preventDefault();
      setIsDragging(false);
      if (status !== "idle") {
        return;
      }
      const entries = await entriesFromDataTransfer(event.dataTransfer);
      if (entries.length > 0) {
        onSelectFiles(entries);
      }
    };

    return (
      <div className="flex flex-col items-center gap-2">
        <input
          accept="*/*"
          className="hidden"
          onChange={handleInputChange}
          ref={inputRef}
          type="file"
        />

        <button
          className={cn(
            "relative flex h-56 w-56 items-center justify-center text-primary-foreground transition-transform",
            status === "idle" && "hover:scale-[1.03]",
            isDragging && "scale-[1.05]"
          )}
          disabled={status !== "idle"}
          onClick={handleClick}
          onDragLeave={handleDragLeave}
          onDragOver={handleDragOver}
          onDrop={handleDrop}
          ref={ref}
          type="button"
        >
          <BlobCanvas
            className="absolute inset-0 h-full w-full"
            progress={status === "idle" ? 0 : progress}
            specs={PRIMARY_CLUSTER_SPECS}
            toColor={PRIMARY_MELT_COLOR}
          />

          <motion.div
            animate={{ scale: 1 }}
            className="relative flex flex-col items-center gap-1.5"
            initial={false}
            key={status}
          >
            {status === "done" ? (
              <Check className="size-6" />
            ) : (
              <Upload className="size-6" />
            )}
            <span className="text-primary-foreground/80 text-xs">
              {status === "idle" && m.upload_button_drop_or_click()}
              {status === "uploading" &&
                m.upload_button_uploading({ progress: Math.round(progress) })}
              {status === "done" && m.upload_button_done()}
            </span>
          </motion.div>
        </button>

        {status === "idle" && (
          <button
            className="text-muted-foreground text-xs underline underline-offset-2 hover:text-foreground"
            onClick={handleChooseFolder}
            type="button"
          >
            {m.upload_button_choose_folder()}
          </button>
        )}
      </div>
    );
  }
);
UploadButton.displayName = "UploadButton";
