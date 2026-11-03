# Security Policy

## Reporting a vulnerability

Please report vulnerabilities privately using GitHub private vulnerability
reporting: open the repository's **Security** tab and choose
**Report a vulnerability**. Do not open a public issue for security problems.

## Scope

In scope: the AI C-Station application (`core/`, `src-tauri/`, `ui/`) and its
build/CI configuration, for example:

- writes to, or reads beyond what is documented from, the Claude Code
  directory (`~/.claude`);
- IPC command argument validation bypasses;
- CSP weakening or any unexpected network access;
- path traversal or command injection through the terminal/folder commands.

Out of scope: vulnerabilities in Claude Code itself, WebView2, or Windows.

## Privacy statement

AI C-Station makes no network requests, sends no telemetry, opens no listening
ports and installs no hooks. It only reads local Claude Code files.

AI C-Station is unofficial; it is not affiliated with or endorsed by
Anthropic. Claude and Claude Code are trademarks of Anthropic.
