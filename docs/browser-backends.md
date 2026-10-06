# Browser backend support

socai controls a browser through a browser-native automation protocol. A
browser extension is not required for the currently supported Chromium path,
and installing an extension does not make an unsupported protocol compatible
with the existing CDP runtime.

| Browser | Protocol path | Current status | Normal-profile reuse |
| --- | --- | --- | --- |
| Google Chrome | Chrome DevTools Protocol (CDP) | Supported | Debuggable existing instance or managed profile |
| Microsoft Edge | Chrome DevTools Protocol (CDP) | Supported through the Chromium backend | Debuggable existing instance or managed profile |
| Firefox | WebDriver BiDi | Not implemented | Must be validated with the future backend |
| Safari | Safari WebDriver | Not implemented | No; Safari uses an isolated Automation window |

“Not implemented” is deliberate: Chrome/Edge verification is not evidence
that Firefox or Safari works. Each backend must pass the same navigation,
identity, input, restoration, and disconnect tests before it is advertised as
supported.

## Chrome and Edge

The existing runtime uses CDP once it has a browser WebSocket endpoint.
Microsoft documents that the Edge DevTools Protocol matches the Chrome
DevTools Protocol API and exposes the same `webSocketDebuggerUrl` discovery
flow. socai already searches Edge executable and profile locations on macOS,
Linux, and Windows.

Chrome 136 and later ignore `--remote-debugging-port` and
`--remote-debugging-pipe` when they target Chrome's default data directory.
The browser must be launched with a non-default `--user-data-dir` (the socai
managed profile is one such directory), or with Chrome for Testing. An
already-running default-profile Chrome cannot be made attachable by setting
`SOCAI_CDP_URL` after launch; that override only selects an endpoint that is
already responding.

To force a managed Edge session, keep the executable override in the
environment of the process that starts the daemon:

```bash
export SOCAI_CHROME_EXECUTABLE="/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"
socai config set chrome.profile managed
socai stop
socai x page_state --pretty
socai status
```

On Windows PowerShell, set the existing compatibility variable to
`msedge.exe` before starting socai:

```powershell
$env:SOCAI_CHROME_EXECUTABLE = "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
socai config set chrome.profile managed
socai stop
socai x page_state --pretty
socai status
```

The variable retains “CHROME” in its name for backward compatibility; its
value may select another compatible Chromium browser. For an already-running
Edge debugging endpoint, use the generic endpoint overrides when starting the
daemon. Plain `http://`/`ws://` endpoints must remain loopback-only. Any remote
endpoint requires authenticated TLS, and its URL (including query parameters)
must be handled as a browser-control secret rather than logged or shared.
socai does not currently validate the endpoint scheme, host, or authentication,
so these restrictions are operator-enforced until the endpoint resolver gains
an explicit security policy:

```bash
socai stop
socai config set chrome.profile existing
SOCAI_CDP_URL=http://127.0.0.1:9222 socai x page_state --pretty
# or: SOCAI_CDP_WS=ws://127.0.0.1:9222/devtools/browser/<id> socai x page_state --pretty
socai status
```

## Firefox boundary

Firefox needs a WebDriver BiDi backend. Mozilla documents WebDriver BiDi as
its bidirectional automation protocol, and Firefox 141 removed the former CDP
selection because BiDi is now the available Remote Agent protocol. Changing
only the executable path cannot work: socai's current `PageSession` sends CDP
`Target`, `Page`, `Runtime`, `DOM`, and `Accessibility` commands.

The future backend must map socai's browser primitives to BiDi browsing
contexts, script evaluation, trusted input actions, screenshots, and lifecycle
events. It must also prove target ownership and reconnection behavior against a
real Firefox release before any site command is enabled.

## Safari boundary

Safari ships `/usr/bin/safaridriver` and implements WebDriver through a local
driver service. It requires Remote Automation to be enabled. Safari's security
model runs automation in a dedicated Automation window isolated from normal
tabs, history, AutoFill, and other browsing data, so it cannot promise the same
signed-in normal-profile behavior as the current Chrome/Edge path.

A Safari backend must use WebDriver navigation, script execution, Actions,
screenshots, and window/session lifecycle. It also needs independent login and
site-consistency acceptance tests; a CDP adapter or executable-path alias is
not sufficient.

Safari 27 additionally exposes an official `safaridriver --mcp` mode for
external agents. That is a possible future bridge, not current socai support:
its tool contract, session isolation, cancellation, and write-action semantics
still need an explicit adapter and the same conformance checks.

## Shared backend contract

This is the required target boundary, not a description of the current Rust
types. Today `NativeSiteAdapter` factories receive the concrete CDP
`PageSession`, and some workflows use CDP-specific node/context identity. A
Firefox or Safari implementation must first put those dependencies behind a
protocol-neutral session interface while keeping site workflow policy shared.
Every backend must then provide equivalent, fail-closed primitives for:

1. owned page/context creation, selection, navigation, back, and close;
2. current URL and document readiness checks;
3. isolated JSON-returning page-script evaluation;
4. trusted pointer, keyboard, and scroll input;
5. screenshot and accessibility/snapshot evidence;
6. disconnect detection, bounded recovery, and cancellation cleanup.

Platform selectors, target identity checks, and write-action policy stay in
socai core and site packages. Protocol adapters transport equivalent
operations; they do not silently weaken or redefine them.

## Optional extension bridge

A browser extension can be added for explicit user-consent UI, tab selection,
or a separately reviewed reduced-capability transport. It is not a generic way
around enterprise policy and is not equivalent across browsers:

| Extension path | Possible role | Limitation |
| --- | --- | --- |
| Chrome/Edge debugger extension | Alternate CDP attachment after explicit tab consent | Still governed by debugger/enterprise policy; not implemented |
| Firefox WebExtension | Consent UI or content-script bridge | No Chrome-compatible debugger API; content scripts do not provide the full trusted-input backend |
| Safari Web Extension | Consent UI or content-script bridge | Separate Apple packaging/review; does not inherit Safari WebDriver capabilities |

Every extension path must satisfy these rules:

- it must not store or forward platform credentials;
- it must use a loopback-only, authenticated, extension-ID/origin-allowlisted
  channel with an explicit user grant for each controlled tab;
- it must not expose a general remote-evaluation endpoint or export cookies,
  authorization headers, or profile secrets;
- it must request the least host permissions needed and reject messages from
  unapproved pages, extensions, or processes;
- it must not duplicate site selectors or platform workflows;
- it must not weaken exact-target checks or one-shot write semantics;
- each Chrome, Edge, Firefox, and Safari package/review track is accepted
  separately.

An extension is not required for Chrome/Edge. If it is later selected as an
alternative Firefox or Safari backend, it must implement the same session
contract and pass that browser's independent conformance and security review;
installing it alone is not evidence of support.

## MCP and remote-control security

CDP, WebDriver BiDi, WebDriver, extensions, and MCP all grant meaningful
control over page data and user sessions. A production adapter must therefore:

- bind unauthenticated development endpoints to loopback only;
- require authenticated TLS for any remote endpoint and redact connection
  URLs, query tokens, cookies, and authorization headers from logs, artifacts,
  and adapter results;
- require an explicit local user action to enable the browser or select a tab;
- declare which page content, screenshots, files, and actions can reach an
  external agent;
- preserve socai's exact-target, one-shot write, cancellation, and audit rules.

Safari's MCP server sends captured browser data directly to the MCP client the
user configured. Depending on the enabled tool, that data can include tab URLs,
DOM/HTML, console output, JavaScript results, screenshots, and network request
or response headers and bodies. A future adapter must default to a locally
launched server, disclose the destination agent/model and its retention
boundary, and filter or redact every result field before it leaves the local
process. It must also allowlist both tools and result fields; tool allowlisting
alone does not prevent an allowed network tool from returning credentials. MCP
availability does not authorize platform writes or remote relays.

## Primary references

- [Microsoft Edge DevTools Protocol](https://learn.microsoft.com/en-us/microsoft-edge/devtools/protocol/)
- [Chrome remote-debugging security notice](https://developer.chrome.com/blog/remote-debugging-port)
- [Chrome extension debugger API](https://developer.chrome.com/docs/extensions/reference/api/debugger)
- [Firefox Remote Protocols](https://firefox-source-docs.mozilla.org/remote/)
- [Firefox remote protocol preferences](https://firefox-source-docs.mozilla.org/remote/Prefs.html)
- [Firefox Remote Agent security](https://firefox-source-docs.mozilla.org/remote/Security.html)
- [Firefox WebExtension incompatibilities](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Chrome_incompatibilities)
- [W3C WebDriver BiDi](https://w3c.github.io/webdriver-bidi/)
- [WebDriver support in Safari](https://webkit.org/blog/6900/webdriver-support-in-safari-10/)
- [Safari MCP server for web developers](https://webkit.org/blog/18136/introducing-the-safari-mcp-server-for-web-developers/)
