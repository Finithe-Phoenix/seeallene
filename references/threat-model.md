# SeeAlln Threat Model (public)

## Goals
- Enable local-only desktop automation (eyes+hands) with strong defaults.

## Non-goals
- Remote control over the internet.
- Bypassing MFA/CAPTCHA.
- Stealth operation.

## Key risks
1) **Accidental clicks outside target**
   - Mitigation: region lock + post-action verification.
2) **Destructive actions** (delete/send/approve)
   - Mitigation: explicit confirmation gate; batch confirmations.
3) **Sensitive data exposure**
   - Mitigation: localhost bind; prefer region lock; optional redaction; avoid logging content by default.
4) **Approval system bypass**
   - Non-goal: Do not bypass platform approvals. Use allowlists for specific safe scripts only.

## Defaults
- Bind: 127.0.0.1
- FPS: 10
- Quality: 60
- No internet exposure
- Logging: minimal metadata unless user opts in
