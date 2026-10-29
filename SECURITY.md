# Security and privacy

This project is a local single-user invoice checker. The HTTP listener binds only
to `127.0.0.1`; do not expose it with a tunnel or reverse proxy as a public service.
Remote hosting and user registration require a separate authenticated gateway,
request limits, HTTPS, storage/retention policy and a deployment security review.

The application does not require an OpenAI API key or a government API key.
Do not put passwords, tokens, signing certificates or receipt photographs in Git.
Use GitHub Actions Secrets or your deployment's secret store when needed later.
Never embed a shared secret in JavaScript, an EXE, a skill or an OpenAPI schema.

Receipt images are decoded in memory and are not saved by the server. Local CLI
and MCP results include invoice fields. Avoid pasting private results in public
issues. Calling the tool through an AI client shares its inputs/results with that
client under that client's privacy policy. Future proxy access logs must redact
invoice query strings.

Updates use HTTPS, a configured manifest directory, no HTTP redirects and SHA-256
verification before decoding/writing. Hashes protect integrity, not authenticity
against compromise of the data repository. Protect the maintainer GitHub account
with MFA/passkeys and never share its credentials. Downloaded data is not executed.

Public source does not make embedded credentials safe. This repository contains
no production credentials. Its proprietary license is retained; public visibility
does not technically prevent others from reading or copying the source.

## Verification on 2026-10-29

- Gitleaks scanned all Git history; no secrets detected.
- cargo-audit: 106 dependencies, no advisories or warnings after rqrr 0.11 update.
- Regression checks cover Unicode QR rejection, nested JSON limits, same-origin
  HTTP requests, MCP size bounds, truncated/forged cloud indices and download URL restrictions.
- Local verification: 85 tests pass; 3 existing official-network tests remain
  opt-in. fmt and clippy pass. Live HTTP smoke tests return 200 for status/check
  and 403 for forged Host and cross-origin update requests.
- Existing distributed Windows EXE is unsigned and predates these fixes. Use
  newly built artifacts from the reviewed commit; do not treat old dist files as
  current releases. Antivirus verification remains pending because local Defender
  reports Product/Feature disabled. No external scanner upload has been performed.
- A rebuild of the old commit was not byte-identical; reproducible build claims
  have not been established. Windows builds now request deterministic linking.

Report suspected vulnerabilities privately to the repository owner using GitHub
private vulnerability reporting when enabled. Never post a real secret in an issue.
