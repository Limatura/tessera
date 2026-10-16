import type { components } from "@tessera/api/schema";
import { describe, expect, it } from "vite-plus/test";

import {
  halfPairedField,
  locationFieldsOf,
  locationRequestOf,
  movesLocation,
} from "./backend-location";

type Backend = components["schemas"]["BackendResponse"];

function backend(
  url: string,
  config: Partial<Backend["config"]> = {}
): Backend {
  return {
    access_key: null,
    capacity_bytes: null,
    config: {
      account: null,
      allow_http: false,
      endpoint: null,
      region: null,
      ...config,
    },
    direct_access: false,
    is_default: false,
    name: "cold",
    reachable: true,
    read_only: false,
    url,
    used_bytes: 0,
  };
}

describe("locationRequestOf", () => {
  it("puts S3 credentials in options, never the URL", () => {
    const fields = {
      ...locationFieldsOf(null),
      accessKeyId: "AKIA",
      bucket: "tessera",
      endpoint: "http://garage:3900",
      kind: "s3" as const,
      prefix: "/uploads/",
      region: "us-east-1",
      secretAccessKey: "secret",
    };
    expect(locationRequestOf(fields)).toEqual({
      options: {
        aws_access_key_id: "AKIA",
        aws_allow_http: "true",
        aws_endpoint: "http://garage:3900",
        aws_region: "us-east-1",
        aws_secret_access_key: "secret",
      },
      url: "s3://tessera/uploads",
    });
  });

  it("leaves blank credentials out, so the stored ones are kept", () => {
    const fields = locationFieldsOf(
      backend("s3://tessera", {
        endpoint: "https://garage.example",
        region: "garage",
      })
    );
    expect(locationRequestOf(fields)).toEqual({
      options: { aws_endpoint: "https://garage.example", aws_region: "garage" },
      url: "s3://tessera",
    });
  });

  it("sends no options for a raw URL, which keeps the stored ones", () => {
    expect(locationRequestOf(locationFieldsOf(backend("memory://")))).toEqual({
      url: "memory://",
    });
  });

  it("puts a WebDAV user and password in the URL only when both are given", () => {
    const fields = {
      ...locationFieldsOf(backend("https://dav.example/files")),
      password: "pw",
      username: "me",
    };
    expect(locationRequestOf(fields).url).toBe(
      "https://me:pw@dav.example/files"
    );
    expect(locationRequestOf({ ...fields, password: "" }).url).toBe(
      "https://dav.example/files"
    );
  });
});

describe("locationFieldsOf", () => {
  it("reads a local path back as it is", () => {
    expect(locationFieldsOf(backend("data/blobs"))).toMatchObject({
      kind: "local",
      path: "data/blobs",
    });
  });

  it("reads an S3 bucket, prefix, endpoint and region back", () => {
    expect(
      locationFieldsOf(
        backend("s3://tessera/a/b", {
          endpoint: "http://garage:3900",
          region: "x",
        })
      )
    ).toMatchObject({
      bucket: "tessera",
      endpoint: "http://garage:3900",
      kind: "s3",
      prefix: "a/b",
      region: "x",
    });
  });
});

describe("locationFieldsOf azure", () => {
  it("reads container@account URLs: container from the username, account from the host", () => {
    expect(
      locationFieldsOf(
        backend("az://media@acme.blob.core.windows.net/uploads/a")
      )
    ).toMatchObject({
      account: "acme",
      bucket: "media",
      kind: "azure",
      prefix: "uploads/a",
    });
  });

  it("prefers the URL's account over the stored one", () => {
    expect(
      locationFieldsOf(
        backend("az://media@acme.blob.core.windows.net", { account: "old" })
      )
    ).toMatchObject({ account: "acme", bucket: "media" });
  });

  it("still reads az://container with the account from options", () => {
    expect(
      locationFieldsOf(backend("az://media/x", { account: "acme" }))
    ).toMatchObject({ account: "acme", bucket: "media", prefix: "x" });
  });
});

describe("halfPairedField", () => {
  it("names the missing half of a key pair", () => {
    const fields = { ...locationFieldsOf(null), accessKeyId: "AKIA" };
    expect(halfPairedField(fields)).toBe("secretAccessKey");
    expect(halfPairedField({ ...fields, accessKeyId: "" })).toBeNull();
  });
});

describe("movesLocation", () => {
  it("counts a new endpoint behind the same bucket as a move", () => {
    const stored = backend("s3://tessera", { endpoint: "http://a:9000" });
    const fields = locationFieldsOf(stored);
    expect(movesLocation(stored, fields)).toBe(false);
    expect(
      movesLocation(stored, { ...fields, endpoint: "http://b:9000" })
    ).toBe(true);
    expect(movesLocation(stored, { ...fields, bucket: "other" })).toBe(true);
  });
});
