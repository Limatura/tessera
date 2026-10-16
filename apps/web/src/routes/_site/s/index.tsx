import { createFileRoute, useNavigate } from "@tanstack/react-router";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { Field, FieldGroup } from "@tessera/ui/components/field";
import {
  InputOTP,
  InputOTPGroup,
  InputOTPSlot,
} from "@tessera/ui/components/input-otp";
import { REGEXP_ONLY_DIGITS_AND_CHARS } from "input-otp";
import { PackageOpen } from "lucide-react";
import { useState } from "react";

import { GlassBackground } from "@/components/glass-background";
import { useCapabilities } from "@/hooks/use-capabilities";
import * as m from "@/paraglide/messages";

export const Route = createFileRoute("/_site/s/")({
  component: RouteComponent,
});

// Used until `/capabilities` answers, and when it fails: the server default.
const DEFAULT_CODE_LENGTH = 8;

function RouteComponent() {
  const navigate = useNavigate();
  const [code, setCode] = useState("");
  const { data: capabilities } = useCapabilities();
  const codeLength = capabilities?.code_length ?? DEFAULT_CODE_LENGTH;

  const goToShare = (value: string) => {
    if (value.length !== codeLength) {
      return;
    }
    navigate({ params: { code: value }, to: "/s/$code" });
  };

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    goToShare(code);
  };

  const handleChange = (value: string) => {
    const upper = value.toUpperCase().slice(0, codeLength);
    setCode(upper);
    if (upper.length === codeLength) {
      goToShare(upper);
    }
  };

  return (
    <>
      <GlassBackground />
      <div className="flex min-h-screen flex-col items-center justify-center gap-4 p-4">
        <Card className="w-full max-w-sm border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
          <CardHeader className="items-center text-center">
            <div className="mb-2 flex size-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
              <PackageOpen className="size-5" />
            </div>
            <CardTitle className="font-medium text-xl tracking-tight">
              {m.share_retrieve_title()}
            </CardTitle>
            <CardDescription>{m.share_retrieve_description()}</CardDescription>
          </CardHeader>
          <CardContent>
            <form onSubmit={handleSubmit}>
              <FieldGroup>
                <Field>
                  <InputOTP
                    aria-label={m.share_code_label()}
                    autoCapitalize="characters"
                    autoFocus
                    autoCorrect="off"
                    inputMode="url"
                    lang="en"
                    key={codeLength}
                    maxLength={codeLength}
                    onChange={handleChange}
                    pattern={REGEXP_ONLY_DIGITS_AND_CHARS}
                    value={code}
                  >
                    <InputOTPGroup className="mx-auto">
                      {Array.from({ length: codeLength }, (_, index) => (
                        <InputOTPSlot index={index} key={index} />
                      ))}
                    </InputOTPGroup>
                  </InputOTP>
                </Field>
              </FieldGroup>
            </form>
          </CardContent>
        </Card>
      </div>
    </>
  );
}
