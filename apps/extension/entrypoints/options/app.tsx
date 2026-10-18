import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@tessera/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { Input } from "@tessera/ui/components/input";
import { Label } from "@tessera/ui/components/label";
import { Switch } from "@tessera/ui/components/switch";
import { toast } from "@tessera/ui/components/toast";
import {
  CircleAlert,
  CircleCheck,
  KeyRound,
  Loader2,
  LogIn,
  LogOut,
} from "lucide-react";
import { useId, useState } from "react";
import type { FormEvent, ReactNode } from "react";

import { i18n } from "#i18n";
import { browser } from "#imports";
import { ExpirySelect } from "@/components/share-choices";
import { toastError } from "@/components/toast-error";
import {
  useAccount,
  useCapabilities,
  useConnection,
} from "@/hooks/use-instance";
import type { Connection } from "@/hooks/use-instance";
import { useStorageItem } from "@/hooks/use-storage";
import { signInWithToken, signOut, startSignIn } from "@/lib/auth";
import {
  fetchCapabilities,
  hasServerPermission,
  normaliseServer,
  requestServerPermission,
} from "@/lib/instance";
import {
  DEFAULT_UPLOAD_DEFAULTS,
  credentialItem,
  defaultsItem,
  serverItem,
} from "@/lib/storage";
import type { UploadDefaults } from "@/lib/storage";

export function App() {
  const connection = useConnection();
  return (
    <main className="mx-auto flex min-h-svh max-w-2xl flex-col gap-4 bg-background px-4 py-8 text-foreground">
      <header className="flex flex-col gap-1">
        <h1 className="text-xl font-semibold tracking-tight">
          {i18n.t("options.title")}
        </h1>
        <p className="text-sm text-muted-foreground">
          {i18n.t("options.subtitle")}
        </p>
      </header>
      <ServerCard connection={connection} />
      {connection.server && (
        <>
          <AccountCard connection={connection} />
          <DefaultsCard connection={connection} />
          <MediaCard />
        </>
      )}
    </main>
  );
}

function Section({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        {description && <CardDescription>{description}</CardDescription>}
      </CardHeader>
      <CardContent className="flex flex-col gap-4">{children}</CardContent>
    </Card>
  );
}

/** A label and its explanation on the left, the control on the right. */
function Row({
  id,
  label,
  description,
  children,
}: {
  id: string;
  label: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <Label htmlFor={id}>{label}</Label>
        {description && (
          <p className="text-xs text-muted-foreground">{description}</p>
        )}
      </div>
      {children}
    </div>
  );
}

/** A save in flight or just refused, shown where the saved state usually is. */
type Attempt =
  | { state: "connecting"; server: string }
  | { state: "failed"; server: string; reason: string };

function ServerCard({ connection }: { connection: Connection }) {
  const queryClient = useQueryClient();
  const capabilities = useCapabilities(connection);
  const { server } = connection;
  const [attempt, setAttempt] = useState<Attempt>();
  const permitted = useQuery({
    enabled: Boolean(server),
    queryFn: () => hasServerPermission(server as string),
    queryKey: ["permission", server],
  });

  const handleRegrant = async () => {
    if (!server) {
      return;
    }
    // Declined, the warning simply stays.
    await requestServerPermission(server);
    await queryClient.invalidateQueries();
  };

  return (
    <Section
      description={i18n.t("options.serverDescription")}
      title={i18n.t("options.serverTitle")}
    >
      {/* Keyed, so a saved address becomes the field's starting value. */}
      <ServerForm
        initial={server ?? ""}
        key={server ?? ""}
        onAttempt={setAttempt}
      />
      <ServerStatus
        attempt={attempt}
        capabilities={capabilities}
        onRegrant={handleRegrant}
        permitted={permitted.data}
        server={server ?? null}
      />
    </Section>
  );
}

/**
 * Where the answer to "is it connected?" lives: under the field it was asked
 * from, until something changes it. A toast in a corner is gone before
 * anyone looks for it.
 */
function ServerStatus({
  attempt,
  capabilities,
  onRegrant,
  permitted,
  server,
}: {
  attempt: Attempt | undefined;
  capabilities: ReturnType<typeof useCapabilities>;
  onRegrant: () => void;
  permitted: boolean | undefined;
  server: string | null;
}) {
  if (attempt?.state === "connecting") {
    return (
      <Status tone="pending">
        {i18n.t("options.serverConnecting", [hostOf(attempt.server)])}
      </Status>
    );
  }
  if (attempt?.state === "failed") {
    return (
      <Status tone="error">
        {i18n.t("options.serverFailedAt", [
          hostOf(attempt.server),
          attempt.reason,
        ])}
      </Status>
    );
  }
  if (!server) {
    return null;
  }
  if (permitted === false) {
    return (
      <Status tone="error">
        {i18n.t("options.serverNoPermission")}{" "}
        <Button onClick={onRegrant} size="xs" variant="outline">
          {i18n.t("options.serverRegrant")}
        </Button>
      </Status>
    );
  }
  if (capabilities.isError) {
    return (
      <Status tone="error">
        {i18n.t("options.serverFailedAt", [
          hostOf(server),
          capabilities.error.message,
        ])}
      </Status>
    );
  }
  if (!capabilities.data) {
    return (
      <Status tone="pending">
        {i18n.t("options.serverConnecting", [hostOf(server)])}
      </Status>
    );
  }
  const { data } = capabilities;
  return (
    <Status
      detail={i18n.t("options.serverDetails", [
        data.version,
        data.encryption
          ? i18n.t("options.encryptionOn")
          : i18n.t("options.encryptionOff"),
        data.anonymous_upload
          ? i18n.t("options.anonymousOn")
          : i18n.t("options.anonymousOff"),
      ])}
      tone="ok"
    >
      {i18n.t("options.serverConnectedTo", [hostOf(server)])}
    </Status>
  );
}

function hostOf(server: string): string {
  return new URL(server).host;
}

function ServerForm({
  initial,
  onAttempt,
}: {
  initial: string;
  /** No argument once the attempt is over and the saved state speaks. */
  onAttempt: (attempt?: Attempt) => void;
}) {
  const queryClient = useQueryClient();
  const inputId = useId();
  const [input, setInput] = useState(initial);
  const [busy, setBusy] = useState(false);

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    const next = normaliseServer(input);
    if (!next) {
      toast.add({ title: i18n.t("options.serverInvalid"), type: "error" });
      return;
    }
    // Asked for before anything is awaited: Firefox only shows a permission
    // prompt while the click that asked for it is still being handled.
    const granted = requestServerPermission(next);
    setBusy(true);
    onAttempt({ server: next, state: "connecting" });
    try {
      await connect(next, granted);
      await queryClient.invalidateQueries();
      // The saved address's own status takes over, green once it answers.
      onAttempt();
    } catch (error) {
      onAttempt({
        reason: failureReason(error),
        server: next,
        state: "failed",
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <form className="flex gap-2" onSubmit={handleSubmit}>
      <Label className="sr-only" htmlFor={inputId}>
        {i18n.t("options.serverTitle")}
      </Label>
      <Input
        autoComplete="url"
        id={inputId}
        inputMode="url"
        onChange={(event) => setInput(event.target.value)}
        placeholder="https://files.example.com"
        value={input}
      />
      <Button disabled={busy || !input.trim()} type="submit">
        {busy && <Loader2 className="animate-spin" data-icon="inline-start" />}
        {busy
          ? i18n.t("options.serverConnectingButton")
          : i18n.t("options.serverSave")}
      </Button>
    </form>
  );
}

/** Why a save failed, in the user's words rather than the parser's. */
function failureReason(error: unknown): string {
  if (error instanceof TypeError) {
    // `fetch` says only "Failed to fetch": nothing answered.
    return i18n.t("errors.network");
  }
  if (error instanceof SyntaxError) {
    // Something answered, with a page rather than Tessera's JSON.
    return i18n.t("options.serverNotTessera");
  }
  return error instanceof Error ? error.message : String(error);
}

/**
 * Saves `server` once the permission is granted and it answers as a Tessera
 * instance. A different instance signs the old account out: its token means
 * nothing to the new one.
 */
async function connect(server: string, granted: Promise<boolean>) {
  if (!(await granted)) {
    throw new Error(i18n.t("options.serverPermissionDenied"));
  }
  await fetchCapabilities(server, null);
  const previous = await serverItem.getValue();
  if (previous !== server && (await credentialItem.getValue())) {
    await signOut();
  }
  await serverItem.setValue(server);
}

async function signIn() {
  try {
    await startSignIn();
  } catch (error) {
    toastError(i18n.t("signIn.failedTitle"), error);
  }
}

async function signOutAndSay() {
  const revoked = await signOut();
  toast.add(
    revoked
      ? { title: i18n.t("signOut.done"), type: "success" }
      : {
          description: i18n.t("signOut.revokeOnSite"),
          title: i18n.t("signOut.done"),
          type: "warning",
        }
  );
}

const HEADLINE = {
  error: "text-destructive",
  ok: "font-medium",
  pending: "text-muted-foreground",
} as const;

function Status({
  tone,
  detail,
  children,
}: {
  tone: "ok" | "pending" | "error";
  /** A quieter second line, under the headline. */
  detail?: string;
  children: ReactNode;
}) {
  const icon = {
    error: <CircleAlert className="mt-px size-4 shrink-0 text-destructive" />,
    ok: <CircleCheck className="mt-px size-4 shrink-0 text-green-500" />,
    pending: (
      <Loader2 className="mt-px size-4 shrink-0 animate-spin text-muted-foreground" />
    ),
  }[tone];
  return (
    <output className="flex items-start gap-2 text-sm">
      {icon}
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className={HEADLINE[tone]}>{children}</span>
        {detail && (
          <span className="text-xs text-muted-foreground">{detail}</span>
        )}
      </div>
    </output>
  );
}

function AccountCard({ connection }: { connection: Connection }) {
  const account = useAccount(connection);
  const tokenId = useId();
  const [token, setToken] = useState("");
  const [showToken, setShowToken] = useState(false);
  const { server, credential } = connection;
  if (!server) {
    return null;
  }

  const handleToken = async (event: FormEvent) => {
    event.preventDefault();
    try {
      const signedIn = await signInWithToken(server, token.trim());
      setToken("");
      setShowToken(false);
      toast.add({
        title: i18n.t("signIn.done", [signedIn.username]),
        type: "success",
      });
    } catch (error) {
      toastError(i18n.t("signIn.failedTitle"), error);
    }
  };

  const lapsed = credential !== null && account.data === null;

  return (
    <Section
      description={i18n.t("options.accountDescription")}
      title={i18n.t("options.accountTitle")}
    >
      {credential ? (
        <div className="flex items-center justify-between gap-4">
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="text-sm font-medium">
              {i18n.t("options.signedInAs", [credential.username])}
            </span>
            {lapsed && (
              <Status tone="error">{i18n.t("popup.signInLapsed")}</Status>
            )}
          </div>
          <div className="flex gap-2">
            {lapsed && (
              <Button onClick={signIn} size="sm">
                <LogIn data-icon="inline-start" />
                {i18n.t("signIn.action")}
              </Button>
            )}
            <Button onClick={signOutAndSay} size="sm" variant="outline">
              <LogOut data-icon="inline-start" />
              {i18n.t("signOut.action")}
            </Button>
          </div>
        </div>
      ) : (
        <>
          <div className="flex items-center justify-between gap-4">
            <p className="text-sm text-muted-foreground">
              {i18n.t("options.signInHint")}
            </p>
            <Button onClick={signIn}>
              <LogIn data-icon="inline-start" />
              {i18n.t("signIn.action")}
            </Button>
          </div>
          {showToken ? (
            <form className="flex flex-col gap-2" onSubmit={handleToken}>
              <Label htmlFor={tokenId}>{i18n.t("options.tokenLabel")}</Label>
              <div className="flex gap-2">
                <Input
                  autoComplete="off"
                  id={tokenId}
                  onChange={(event) => setToken(event.target.value)}
                  placeholder="tsk_…"
                  type="password"
                  value={token}
                />
                <Button disabled={!token.trim()} type="submit">
                  {i18n.t("options.tokenUse")}
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">
                {i18n.t("options.tokenHint")}{" "}
                <a
                  className="underline underline-offset-2"
                  href={`${server}/user`}
                  rel="noopener"
                  target="_blank"
                >
                  {i18n.t("options.tokenPage")}
                </a>
              </p>
            </form>
          ) : (
            <Button
              className="self-start"
              onClick={() => setShowToken(true)}
              size="xs"
              variant="link"
            >
              <KeyRound data-icon="inline-start" />
              {i18n.t("options.tokenInstead")}
            </Button>
          )}
        </>
      )}
    </Section>
  );
}

function DefaultsCard({ connection }: { connection: Connection }) {
  const defaults = useStorageItem(defaultsItem) ?? DEFAULT_UPLOAD_DEFAULTS;
  const capabilities = useCapabilities(connection);
  const ids = {
    burn: useId(),
    copy: useId(),
    encrypt: useId(),
    expiry: useId(),
    notify: useId(),
  };

  const change = async (patch: Partial<UploadDefaults>) => {
    await defaultsItem.setValue({ ...defaults, ...patch });
  };

  return (
    <Section
      description={i18n.t("options.defaultsDescription")}
      title={i18n.t("options.defaultsTitle")}
    >
      {/* Only where the operator turned encryption on: elsewhere it does nothing. */}
      {capabilities.data?.encryption && (
        <Row
          description={i18n.t("options.encryptDescription")}
          id={ids.encrypt}
          label={i18n.t("options.encryptLabel")}
        >
          <Switch
            checked={defaults.encrypt}
            id={ids.encrypt}
            onCheckedChange={(encrypt) => change({ encrypt })}
          />
        </Row>
      )}
      <Row
        description={i18n.t("options.expiryDescription")}
        id={ids.expiry}
        label={i18n.t("options.expiryLabel")}
      >
        <ExpirySelect
          capabilities={capabilities.data}
          id={ids.expiry}
          onChange={(expiresInHours) => change({ expiresInHours })}
          value={defaults.expiresInHours}
        />
      </Row>
      <Row
        description={i18n.t("options.burnDescription")}
        id={ids.burn}
        label={i18n.t("options.burnLabel")}
      >
        <Switch
          checked={defaults.burnAfterReading}
          id={ids.burn}
          onCheckedChange={(burnAfterReading) => change({ burnAfterReading })}
        />
      </Row>
      <Row id={ids.copy} label={i18n.t("options.copyLabel")}>
        <Switch
          checked={defaults.copyLink}
          id={ids.copy}
          onCheckedChange={(copyLink) => change({ copyLink })}
        />
      </Row>
      <Row id={ids.notify} label={i18n.t("options.notifyLabel")}>
        <Switch
          checked={defaults.notify}
          id={ids.notify}
          onCheckedChange={(notify) => change({ notify })}
        />
      </Row>
    </Section>
  );
}

/** Every site, for fetching media the clicked page's own permission cannot reach. */
const ANY_SITE = { origins: ["http://*/*", "https://*/*"] };

function MediaCard() {
  const queryClient = useQueryClient();
  const switchId = useId();
  const allowed = useQuery({
    queryFn: () => browser.permissions.contains(ANY_SITE),
    queryKey: ["permission", "any-site"],
  });

  const handleChange = async (checked: boolean) => {
    // Asked for straight from the click, for the same reason as the server's.
    // Declined, the switch goes on showing what is actually granted.
    await (checked
      ? browser.permissions.request(ANY_SITE)
      : browser.permissions.remove(ANY_SITE));
    await queryClient.invalidateQueries({ queryKey: ["permission"] });
  };

  return (
    <Section
      description={i18n.t("options.mediaDescription")}
      title={i18n.t("options.mediaTitle")}
    >
      <Row id={switchId} label={i18n.t("options.mediaLabel")}>
        <Switch
          checked={allowed.data ?? false}
          id={switchId}
          onCheckedChange={handleChange}
        />
      </Row>
    </Section>
  );
}
