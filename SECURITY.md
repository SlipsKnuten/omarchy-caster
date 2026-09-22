# Security policy

Caster is pre-release software. Report security or privacy issues privately to
the repository owner once the public repository is available; do not attach
session logs without redacting window titles, receiver names, IDs, and network
addresses.

The application never intentionally records media. Window and application-audio
scope must never widen implicitly. Treat failures that leak another window,
route unrelated audio, leave capture running, or execute display data as code as
security issues.
