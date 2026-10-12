# Security policy

Tessera holds other people's files, often from people its operator does not know. Please report security problems privately, not in a public issue.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting: open the **Security** tab of this repository and choose **Report a vulnerability**. Only the maintainers see the report.

Include what you can of:

- what an attacker can do, and what they need first (an account, an administrator's click, a particular setting);
- the version or commit, and the relevant settings;
- steps to reproduce, or a proof of concept.

We will reply within a week. Once a fix is released we publish an advisory and credit you, unless you would rather not be named.

## Supported versions

Only the latest release receives fixes. Self-hosters should upgrade rather than expect a backport.

## Scope

In scope: the server, the web app, the CLI, the browser extension and the Docker image built from this repository.

Out of scope:

- A deployment's own configuration, such as a reverse proxy that strips headers or a bucket left public.
- What the operator can see. Without end-to-end encryption the operator can read uploads; that is the design, not a flaw. See `docs/encryption.md`.
- Disk filling on an instance that allows anonymous uploads with no `storage.max_total_bytes` set. The server warns about this at startup.
