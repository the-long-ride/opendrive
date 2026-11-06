# Security Policy

## Reporting

Report vulnerabilities privately via GitHub Security Advisories.
Do not open public issues for security bugs.

## Scope

- Token/keychain handling (`opendrive-auth`)
- Path traversal in mounts / staging
- OAuth loopback binding (must stay on 127.0.0.1)
- Dependency advisories (`cargo deny` / CI)

## Non-goals (v1)

- Hardening against a compromised local user
- Formal audit
