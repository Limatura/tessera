import { describe, expect, it } from "vite-plus/test";

import { backendUrlProblem } from "./storage-backend-url";

describe("backendUrlProblem", () => {
  it.each([
    "",
    "/var/lib/tessera/blobs",
    "./blobs",
    "file:///data/blobs",
    "memory://",
    "s3://my-bucket",
    "s3://my-bucket/tessera",
    "gs://my_bucket/prefix",
    "az://my-container/prefix",
    "abfss://my-container@acct.dfs.core.windows.net/path",
    "https://dav.example.com/remote.php/dav",
  ])("accepts %j", (url) => {
    expect(backendUrlProblem(url)).toBeNull();
  });

  it.each([
    "s3://",
    "s3://ab",
    "s3://My-Bucket",
    "s3://-bucket",
    "s3://key@my-bucket",
    "s3://key:@my-bucket",
    // object_store keeps only the bucket and drops the keys without a word.
    "s3://AKIA:secret@my-bucket/tessera",
    "s3://my bucket",
    "gs://UPPER",
    "gs://key:secret@my-bucket",
    "az://x",
    "abfs://acct.dfs.core.windows.net/path",
    "file://data",
    "file:///",
    "ftp://host/dir",
    "https://",
  ])("rejects %j", (url) => {
    expect(backendUrlProblem(url)).not.toBeNull();
  });
});
