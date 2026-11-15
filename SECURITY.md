# Security Policy

## Supported versions

Only the latest release receives fixes.

## Reporting a vulnerability

Please **do not open a public issue**. Report it privately, either:

* through GitHub: *Security* tab → *Report a vulnerability*, or
* by email to **amandagimendes@duck.com**.

You will get an answer within 7 days.

Of particular interest:

* anything that makes BIA perform network I/O (a breach of [docs/SEALED.md](docs/SEALED.md)),
* file corruption or data loss on save, rename or delete,
* path traversal through explorer names or the command line,
* crashes triggered by opening a crafted file, theme or grammar.
