import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { Button } from "@tessera/ui/components/button";
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
import * as m from "@/paraglide/messages";

export const Route = createFileRoute("/_site/s/")({
  component: RouteComponent,
});

const CODE_LENGTH = 8;

function RouteComponent() {
  const navigate = useNavigate();
  const [code, setCode] = useState("");

  const goToShare = (value: string) => {
    if (value.length !== CODE_LENGTH) {
      return;
    }
    navigate({ params: { code: value }, to: "/s/$code" });
  };

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    goToShare(code);
  };

  const handleChange = (value: string) => {
    const upper = value.toUpperCase();
    setCode(upper);
    if (upper.length === CODE_LENGTH) {
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
                    inputMode="text"
                    maxLength={CODE_LENGTH}
                    onChange={handleChange}
                    pattern={REGEXP_ONLY_DIGITS_AND_CHARS}
                    value={code}
                  >
                    <InputOTPGroup className="mx-auto">
                      {Array.from({ length: CODE_LENGTH }, (_, index) => (
                        <InputOTPSlot index={index} key={index} />
                      ))}
                    </InputOTPGroup>
                  </InputOTP>
                </Field>
                <Button
                  className="w-full"
                  disabled={code.length !== CODE_LENGTH}
                  type="submit"
                >
                  {m.share_code_continue()}
                </Button>
              </FieldGroup>
            </form>
          </CardContent>
        </Card>
      </div>
    </>
  );
}
