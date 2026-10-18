import { canAccessAdmin } from "@tessera/auth/permissions";
import { Button } from "@tessera/ui/components/button";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@tessera/ui/components/tabs";
import { Textarea } from "@tessera/ui/components/textarea";
import {
  CircleAlert,
  FileText,
  Globe,
  LayoutDashboard,
  LockKeyhole,
  LogIn,
  Settings,
  Upload,
} from "lucide-react";
import { useState } from "react";
import type { FormEvent, ReactNode } from "react";

import { i18n } from "#i18n";
import { browser } from "#imports";
import { JobList } from "@/components/job-list";
import { ShareList } from "@/components/share-list";
import { toastError } from "@/components/toast-error";
import {
  useAccount,
  useCapabilities,
  useConnection,
} from "@/hooks/use-instance";
import type { Connection } from "@/hooks/use-instance";
import { useJobs, useStorageItem } from "@/hooks/use-storage";
import { startSignIn } from "@/lib/auth";
import { expiryLabel } from "@/lib/format";
import { isActive } from "@/lib/jobs";
import { send } from "@/lib/messages";
import { defaultsItem } from "@/lib/storage";

export function App() {
  const connection = useConnection();

  if (connection.server === undefined) {
    return <Frame />;
  }
  if (connection.server === null) {
    return (
      <Frame>
        <div className="flex flex-col items-center gap-3 px-6 py-10 text-center">
          <p className="text-sm font-medium">{i18n.t("popup.noServer")}</p>
          <p className="text-xs text-muted-foreground">
            {i18n.t("popup.noServerHint")}
          </p>
          <Button onClick={openOptions}>
            <Settings data-icon="inline-start" />
            {i18n.t("popup.openSettings")}
          </Button>
        </div>
      </Frame>
    );
  }
  return <Connected connection={connection} />;
}

function Connected({ connection }: { connection: Connection }) {
  const [tab, setTab] = useState("jobs");
  const [writing, setWriting] = useState(false);
  const activeCount = useJobs().filter(isActive).length;

  const handleNoteSent = () => {
    setWriting(false);
    setTab("jobs");
  };

  return (
    <Frame connection={connection}>
      <AccountBar connection={connection} />
      <UploadNotice connection={connection} />
      <div className="grid grid-cols-2 gap-2 px-3 pt-3">
        <Button onClick={() => openTab(connection.server as string)}>
          <Upload data-icon="inline-start" />
          {i18n.t("popup.uploadFiles")}
        </Button>
        <Button
          aria-expanded={writing}
          onClick={() => setWriting((open) => !open)}
          variant="outline"
        >
          <FileText data-icon="inline-start" />
          {i18n.t("popup.uploadText")}
        </Button>
      </div>
      {writing && <NoteForm onSent={handleNoteSent} />}
      <Tabs
        className="min-h-0 flex-1 px-3 pt-3"
        onValueChange={setTab}
        value={tab}
      >
        <TabsList className="w-full">
          <TabsTrigger value="jobs">
            {activeCount > 0
              ? i18n.t("popup.tabJobsActive", [activeCount])
              : i18n.t("popup.tabJobs")}
          </TabsTrigger>
          <TabsTrigger value="shares">{i18n.t("popup.tabShares")}</TabsTrigger>
        </TabsList>
        <TabsContent className="max-h-80 overflow-y-auto pb-3" value="jobs">
          <JobList />
        </TabsContent>
        <TabsContent className="max-h-80 overflow-y-auto pb-3" value="shares">
          <ShareList connection={connection} />
        </TabsContent>
      </Tabs>
      <DefaultsSummary />
    </Frame>
  );
}

function Frame({
  connection,
  children,
}: {
  connection?: Connection;
  children?: ReactNode;
}) {
  return (
    <main className="flex min-h-48 w-[380px] flex-col bg-background text-foreground">
      <header className="flex items-center gap-1 border-b px-3 py-2">
        <img
          alt=""
          className="mr-1 size-5"
          height={20}
          src="/icon/32.png"
          width={20}
        />
        <h1 className="flex-1 text-sm font-semibold tracking-tight">
          {i18n.t("extName")}
        </h1>
        {connection?.server && <SiteLinks connection={connection} />}
        <Button
          aria-label={i18n.t("popup.openSettings")}
          onClick={openOptions}
          size="icon-sm"
          title={i18n.t("popup.openSettings")}
          variant="ghost"
        >
          <Settings />
        </Button>
      </header>
      {children}
    </main>
  );
}

/**
 * The site itself, and its admin panel for an account that can open any of
 * it. The same test as the site's own header (`canAccessAdmin`): the token's
 * session reports the account's permissions, not the token's narrower ones.
 */
function SiteLinks({ connection }: { connection: Connection }) {
  const account = useAccount(connection);
  const server = connection.server as string;
  return (
    <>
      <Button
        aria-label={i18n.t("popup.openSite")}
        onClick={() => openTab(server)}
        size="icon-sm"
        title={i18n.t("popup.openSite")}
        variant="ghost"
      >
        <Globe />
      </Button>
      {canAccessAdmin(account.data) && (
        <Button
          aria-label={i18n.t("popup.openAdmin")}
          onClick={() => openTab(`${server}/admin`)}
          size="icon-sm"
          title={i18n.t("popup.openAdmin")}
          variant="ghost"
        >
          <LayoutDashboard />
        </Button>
      )}
    </>
  );
}

function AccountBar({ connection }: { connection: Connection }) {
  const account = useAccount(connection);
  const host = connection.server ? new URL(connection.server).host : "";
  // A token the instance no longer takes (revoked on the site, expired).
  const lapsed = connection.credential !== null && account.data === null;

  return (
    <div className="flex items-center gap-2 px-3 pt-3 text-xs">
      <span
        className="truncate text-muted-foreground"
        title={connection.server ?? ""}
      >
        {host}
      </span>
      <span aria-hidden className="text-muted-foreground">
        ·
      </span>
      {connection.credential && !lapsed ? (
        <span className="flex min-w-0 flex-1 items-center gap-1">
          <span className="truncate font-medium">
            {connection.credential.username}
          </span>
          <EncryptionBadge connection={connection} />
        </span>
      ) : (
        <>
          <span className="flex min-w-0 flex-1 items-center gap-1">
            <span className="truncate text-muted-foreground">
              {lapsed
                ? i18n.t("popup.signInLapsed")
                : i18n.t("popup.anonymous")}
            </span>
            <EncryptionBadge connection={connection} />
          </span>
          <Button onClick={signIn} size="xs" variant="outline">
            <LogIn data-icon="inline-start" />
            {i18n.t("signIn.action")}
          </Button>
        </>
      )}
    </div>
  );
}

/**
 * Green, as the site marks encryption, when uploads from here are sealed by
 * default: the setting is on and the instance allows it, which is exactly
 * when an upload would be.
 */
function EncryptionBadge({ connection }: { connection: Connection }) {
  const defaults = useStorageItem(defaultsItem);
  const capabilities = useCapabilities(connection);
  if (!defaults?.encrypt || capabilities.data?.encryption !== true) {
    return null;
  }
  const label = i18n.t("popup.encryptingDefault");
  return (
    <span className="flex shrink-0 text-green-500" title={label}>
      <LockKeyhole aria-hidden className="size-3.5" />
      <span className="sr-only">{label}</span>
    </span>
  );
}

/** Says so up front when this instance will refuse an upload from here. */
function UploadNotice({ connection }: { connection: Connection }) {
  const capabilities = useCapabilities(connection);
  let message: string | undefined;
  if (capabilities.isError) {
    message = i18n.t("popup.unreachable", [capabilities.error.message]);
  } else if (
    capabilities.data &&
    !connection.credential &&
    !capabilities.data.anonymous_upload
  ) {
    message = i18n.t("errors.signInRequired");
  }
  if (!message) {
    return null;
  }
  return (
    <p className="mx-3 mt-3 flex items-start gap-1.5 rounded-md bg-destructive/10 px-2.5 py-2 text-xs text-destructive">
      <CircleAlert className="mt-px size-3.5 shrink-0" />
      <span>{message}</span>
    </p>
  );
}

function NoteForm({ onSent }: { onSent: () => void }) {
  const [text, setText] = useState("");

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    if (!text.trim()) {
      return;
    }
    // Shown at once, not after the background answers: by then the user may
    // have moved to another tab, and switching back under them is wrong.
    setText("");
    onSent();
    try {
      await send({ text, type: "upload-note" });
    } catch (error) {
      toastError(i18n.t("errors.unknownTitle"), error);
    }
  };

  return (
    <form className="flex flex-col gap-2 px-3 pt-3" onSubmit={handleSubmit}>
      <Textarea
        aria-label={i18n.t("popup.uploadText")}
        autoFocus
        className="max-h-48 min-h-24 font-mono text-xs"
        onChange={(event) => setText(event.target.value)}
        placeholder={i18n.t("popup.notePlaceholder")}
        value={text}
      />
      <Button
        className="self-end"
        disabled={!text.trim()}
        size="sm"
        type="submit"
      >
        {i18n.t("popup.noteSubmit")}
      </Button>
    </form>
  );
}

/** What a right-click upload will do, so nobody is surprised by it. */
function DefaultsSummary() {
  const defaults = useStorageItem(defaultsItem);
  if (!defaults) {
    return null;
  }
  const parts = [
    defaults.expiresInHours === null
      ? i18n.t("expiry.instance")
      : expiryLabel(defaults.expiresInHours),
    ...(defaults.burnAfterReading ? [i18n.t("options.burnLabel")] : []),
  ];
  return (
    <button
      className="flex items-center gap-1.5 border-t px-3 py-2 text-left text-xs text-muted-foreground hover:bg-muted/50"
      onClick={openOptions}
      type="button"
    >
      <span className="truncate">
        {i18n.t("popup.defaults", [parts.join(" · ")])}
      </span>
    </button>
  );
}

async function openTab(url: string) {
  await browser.tabs.create({ url });
  window.close();
}

async function openOptions() {
  await browser.runtime.openOptionsPage();
}

async function signIn() {
  try {
    await startSignIn();
    // The sign-in finishes in the tab just opened.
    window.close();
  } catch (error) {
    toastError(i18n.t("signIn.failedTitle"), error);
  }
}
