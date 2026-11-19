import { zodResolver } from "@hookform/resolvers/zod";
import { Link } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import { FieldGroup } from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { Gauge, Info } from "lucide-react";
import { useMemo } from "react";
import { Controller } from "react-hook-form";
import type {
  ControllerFieldState,
  ControllerRenderProps,
} from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import { bytesToMbps, mbpsToBytes, suggestedMbps } from "@/lib/admin/bandwidth";
import { bandwidthSettingsSchema } from "@/lib/admin/settings-schema";
import type { BandwidthSettingsValues } from "@/lib/admin/settings-schema";
import { asNumber } from "@/lib/admin/settings-values";
import { loadLastSpeedTest } from "@/lib/speed-test-last";
import type { LastSpeedTest } from "@/lib/speed-test-last";
import * as m from "@/paraglide/messages";

import { SettingRow, SettingsNotice } from "./form-fields";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";
import { SettingsCard } from "./settings-page";

type Settings = components["schemas"]["SettingsResponse"];
type FieldName = keyof BandwidthSettingsValues;

const KEYS = {
  download: "bandwidth.download",
  downloadPerTransfer: "bandwidth.download_per_transfer",
  upload: "bandwidth.upload",
  uploadPerTransfer: "bandwidth.upload_per_transfer",
} satisfies SettingsKeyMap<BandwidthSettingsValues>;

const ENCODERS = {
  download: mbpsToBytes,
  downloadPerTransfer: mbpsToBytes,
  upload: mbpsToBytes,
  uploadPerTransfer: mbpsToBytes,
};

export function BandwidthSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <BandwidthSettingsFields defaultValues={initialValues(settings)} />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): BandwidthSettingsValues {
  const read = (key: string) =>
    bytesToMbps(asNumber(readSetting(settings, key), 0));
  return {
    download: read(KEYS.download),
    downloadPerTransfer: read(KEYS.downloadPerTransfer),
    upload: read(KEYS.upload),
    uploadPerTransfer: read(KEYS.uploadPerTransfer),
  };
}

function BandwidthSettingsFields({
  defaultValues,
}: {
  defaultValues: BandwidthSettingsValues;
}) {
  const form = useSettingsForm<BandwidthSettingsValues>({
    defaultValues,
    encoders: ENCODERS,
    keys: KEYS,
    resolver: zodResolver(bandwidthSettingsSchema),
  });
  // Read once: a test run on another tab shows up on the next visit.
  const lastTest = useMemo(() => loadLastSpeedTest(), []);
  const limited = Object.values(defaultValues).some((value) => value > 0);

  const handleApplySuggestion = (test: LastSpeedTest) => {
    const options = { shouldDirty: true, shouldValidate: true };
    form.setValue("download", suggestedMbps(test.downloadMbps), options);
    form.setValue("upload", suggestedMbps(test.uploadMbps), options);
  };

  return (
    <form className="flex flex-col gap-6" onSubmit={form.submit}>
      <SettingsCard
        description={m.settings_bandwidth_suggest_description()}
        title={m.settings_bandwidth_suggest_title()}
      >
        <div className="flex flex-col gap-4">
          {lastTest ? (
            <div className="flex flex-col gap-3 @md:flex-row @md:items-center @md:justify-between">
              <p className="text-sm">
                {m.settings_bandwidth_suggest_measured({
                  download: lastTest.downloadMbps.toFixed(1),
                  upload: lastTest.uploadMbps.toFixed(1),
                })}
                <br />
                <span className="text-muted-foreground">
                  {m.settings_bandwidth_suggest_values({
                    download: suggestedMbps(lastTest.downloadMbps),
                    upload: suggestedMbps(lastTest.uploadMbps),
                  })}
                </span>
              </p>
              <div className="flex shrink-0 gap-2">
                <Button
                  nativeButton={false}
                  render={<Link to="/admin/speed" />}
                  size="sm"
                  variant="outline"
                >
                  <Gauge />
                  {m.settings_bandwidth_suggest_rerun()}
                </Button>
                <Button
                  onClick={() => handleApplySuggestion(lastTest)}
                  size="sm"
                  type="button"
                >
                  {m.settings_bandwidth_suggest_apply()}
                </Button>
              </div>
            </div>
          ) : (
            <div className="flex flex-col gap-3 @md:flex-row @md:items-center @md:justify-between">
              <p className="text-muted-foreground text-sm">
                {m.settings_bandwidth_suggest_none()}
              </p>
              <Button
                nativeButton={false}
                render={<Link to="/admin/speed" />}
                size="sm"
                variant="outline"
              >
                <Gauge />
                {m.settings_bandwidth_suggest_run()}
              </Button>
            </div>
          )}
          <SettingsNotice icon={<Info />}>
            <p>{m.settings_bandwidth_suggest_headroom()}</p>
            <p>{m.settings_bandwidth_suggest_caveat()}</p>
            {limited && <p>{m.settings_bandwidth_suggest_limited()}</p>}
          </SettingsNotice>
        </div>
      </SettingsCard>

      <SettingsCard
        description={m.settings_bandwidth_server_description()}
        title={m.settings_bandwidth_server_title()}
      >
        <FieldGroup>
          <MbpsController
            description={m.settings_bandwidth_download_description()}
            form={form}
            label={m.settings_bandwidth_download_label()}
            name="download"
          />
          <MbpsController
            description={m.settings_bandwidth_upload_description()}
            form={form}
            label={m.settings_bandwidth_upload_label()}
            name="upload"
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_bandwidth_transfer_description()}
        title={m.settings_bandwidth_transfer_title()}
      >
        <FieldGroup>
          <MbpsController
            description={m.settings_bandwidth_download_per_transfer_description()}
            form={form}
            label={m.settings_bandwidth_download_per_transfer_label()}
            name="downloadPerTransfer"
          />
          <MbpsController
            description={m.settings_bandwidth_upload_per_transfer_description()}
            form={form}
            label={m.settings_bandwidth_upload_per_transfer_label()}
            name="uploadPerTransfer"
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsFormFooter form={form} />
    </form>
  );
}

function MbpsController({
  form,
  name,
  label,
  description,
}: {
  form: ReturnType<typeof useSettingsForm<BandwidthSettingsValues>>;
  name: FieldName;
  label: string;
  description: string;
}) {
  return (
    <Controller
      control={form.control}
      name={name}
      render={({ field, fieldState }) => (
        <MbpsField
          description={description}
          field={field}
          fieldState={fieldState}
          id={`bandwidth-${name}`}
          label={label}
        />
      )}
    />
  );
}

/** A rate in Mbit/s; empty or 0 is unlimited. */
function MbpsField({
  field,
  fieldState,
  id,
  label,
  description,
}: {
  field: ControllerRenderProps<BandwidthSettingsValues, FieldName>;
  fieldState: ControllerFieldState;
  id: string;
  label: string;
  description: string;
}) {
  const { value } = field;
  return (
    <SettingRow
      description={description}
      error={fieldState.invalid ? fieldState.error : undefined}
      id={id}
      label={label}
    >
      <div className="relative">
        <Input
          {...field}
          aria-invalid={fieldState.invalid}
          className="pr-16 tabular-nums"
          id={id}
          min={0}
          onChange={(event) => {
            const next = event.target.valueAsNumber;
            field.onChange(Number.isNaN(next) ? 0 : next);
          }}
          placeholder={m.settings_bandwidth_unlimited()}
          step="any"
          type="number"
          value={value === 0 ? "" : value}
        />
        <span className="pointer-events-none absolute inset-y-0 right-3 flex items-center text-muted-foreground text-sm">
          {m.settings_bandwidth_unit()}
        </span>
      </div>
    </SettingRow>
  );
}
