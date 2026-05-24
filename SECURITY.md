# Security Policy

Please report vulnerabilities privately by opening a GitHub security advisory or
emailing the repository maintainer.

This module rewrites request metadata before cache lookup. Review policy changes
carefully because over-normalization can merge responses that should remain
separate.

Recommended production posture:

- Start in `report` mode and compare `X-Cache-Key` across real traffic.
- Only remove query parameters known not to affect origin responses.
- Keep authentication, session, preview, and personalization parameters out of
  remove/ignore lists unless VCL also bypasses cache for those requests.
- Use Varnish VCL for final cache/pass decisions.
