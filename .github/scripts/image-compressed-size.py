#!/usr/bin/env python3
"""Sum what a `docker pull` of this image would actually transfer.

`docker image inspect --format '{{.Size}}'` is not that number, and is not a
number worth putting in a table at all. Moby's own tracker has never settled
what the field means — moby#43862 describes it as "the size of the Manifest
(and compressed layers???)", question marks included — and the containerd-store
implementation has been rewritten repeatedly through moby#49535, #49707, #52482
and #52901, with users reporting the reported size doubling or tripling across
an upgrade in moby#51579 and #50103. Today it is `PresentContentSize +
snapshotUsage`, packed and unpacked content added together, which is a disk
footprint rather than either a download or a filesystem size. A figure that
changes meaning with the daemon version cannot be compared against last week's
build.

So the number is measured here instead, from `docker save`: every layer,
gzipped, added up. gzip level 6 because that is what containerd uses — its
compression package calls `gzip.NewWriter(dest)` with no level, which is Go's
`DefaultCompression` — so this lands close to what the registry will really
hold. Close, not exact: it is here to catch a regression, not to be quoted as a
contract.

`docker save` may hand over layers compressed or not depending on the image
store, so each one is checked for the gzip magic bytes and only compressed if it
is not already.
"""

import gzip
import json
import sys
import tarfile

# Big enough that a 35 MB layer is a handful of reads, small enough not to hold
# the whole image in memory on a runner that is already close to its disk floor.
CHUNK = 1 << 20
GZIP_MAGIC = b"\x1f\x8b"


def compressed_size(stream) -> int:
    """Bytes this layer would occupy in a registry."""
    head = stream.read(len(GZIP_MAGIC))
    if head.startswith(GZIP_MAGIC):
        # Already a compressed blob: recompressing would measure the wrong thing.
        total = len(head)
        while chunk := stream.read(CHUNK):
            total += len(chunk)
        return total

    total = 0

    class Counter:
        def write(self, data: bytes) -> int:
            nonlocal total
            total += len(data)
            return len(data)

    # mtime=0 so the count does not wobble by a few bytes between runs.
    with gzip.GzipFile(fileobj=Counter(), mode="wb", compresslevel=6, mtime=0) as out:
        out.write(head)
        while chunk := stream.read(CHUNK):
            out.write(chunk)
    return total


def main() -> int:
    archive = sys.argv[1]

    with tarfile.open(archive, mode="r") as tar:
        manifest_member = tar.extractfile("manifest.json")
        if manifest_member is None:
            print("manifest.json missing from the archive", file=sys.stderr)
            return 1
        manifest = json.load(manifest_member)

        # `docker save` writes a list with one entry per saved tag. This script
        # is only ever handed a single tag.
        layers = manifest[0]["Layers"]

        total = 0
        for name in layers:
            member = tar.extractfile(name)
            if member is None:
                print(f"layer {name} missing from the archive", file=sys.stderr)
                return 1
            total += compressed_size(member)

    print(total)
    return 0


if __name__ == "__main__":
    sys.exit(main())
