#!/usr/bin/env python3
"""Regenerates `docs/schema/` from the migrations.

# Why the migrations and not a running server

The tables this documents are created by the files in
`apps/server/src/db/migration/sql/`, so replaying those is the shortest path
from source of truth to diagram — no build, no server to start and stop, no
port to be already in use. It was checked against a database the server itself
migrated: same tables, same indexes, byte-identical definitions.

# Why SQLite

Any of the three dialects would draw the same picture — the tables and the keys
between them are the same everywhere. SQLite is the one that needs nothing
running.
"""

import glob
import pathlib
import shutil
import sqlite3
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
MIGRATIONS = ROOT / "apps/server/src/db/migration/sql"
OUT = ROOT / "docs/schema"
IMAGE = "ghcr.io/k1low/tbls:v1.85.4"


def build_database(path: pathlib.Path) -> None:
    connection = sqlite3.connect(path)
    connection.execute("PRAGMA foreign_keys = ON")
    for file in sorted(glob.glob(str(MIGRATIONS / "*.sqlite.sql"))):
        connection.executescript(pathlib.Path(file).read_text(encoding="utf-8"))
    connection.commit()
    connection.close()


def main() -> int:
    if not shutil.which("docker"):
        print("docker is needed to run tbls", file=sys.stderr)
        return 1

    with tempfile.TemporaryDirectory() as work_str:
        work = pathlib.Path(work_str)
        build_database(work / "schema.db")
        shutil.copy(ROOT / ".tbls.yml", work / ".tbls.yml")

        result = subprocess.run(
            [
                "docker", "run", "--rm",
                # Written as the invoking user, or the output arrives owned by
                # root and the next run cannot overwrite it.
                "--user", f"{__import__('os').getuid()}:{__import__('os').getgid()}",
                "-v", f"{work}:/work", "-w", "/work", IMAGE,
                "doc", "--rm-dist",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            print(result.stdout + result.stderr, file=sys.stderr)
            return result.returncode

        if OUT.exists():
            shutil.rmtree(OUT)
        shutil.copytree(work / "out", OUT)

    # `schema.json` is tbls's own machine-readable dump. It carries the path of
    # the temporary database it was built from, so it changes on every run for
    # a reason that has nothing to do with the schema — which would make the
    # drift check fire constantly and teach everyone to ignore it.
    (OUT / "schema.json").unlink(missing_ok=True)
    print(f"wrote {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
