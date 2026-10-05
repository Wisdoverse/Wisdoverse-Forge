# Frontend And Shared Contracts

Read this guide for React, browser behavior, and TypeScript contracts.

## Feature-Sliced Design

The active app is `src/app`.
Its layer order is `app -> pages -> widgets -> features -> entities -> shared`.

- Keep imports within the owning slice or directed toward lower layers.
- Limit feature imports to that feature, `entities`, and `shared`.
- Move cross-feature composition to `widgets`.
- Move reusable domain or common code to `entities` or `shared`, respectively.
- Before using retired frontend code, adapt it into the correct `src/app` layer.

| Location under `src/app` | Ownership |
| --- | --- |
| `shared` | Shared utilities, global contexts, API clients independent of generated code, and stores used across slices |
| `entities` | Domain API clients and types |
| `features` | User workflows |
| `widgets`, `pages` | Composed route surfaces |
| `routes`, `layouts`, `providers` | App wiring |
| `hooks` | WebSocket dispatch |
| `features/*/model` | Realtime reducers for the owning feature |

- Compose new views through `routes`, `pages`, or `widgets`.
- Do not restore a root-level view registry.
- Run `npm run fsd:check` for every frontend change.
- Keep this check in `npm run lint`.

## Contracts And Browser Checks

- Keep API clients, WebSocket handlers, and Rust serializers consistent with `shared/types/`.
- For CLI protocol changes, update the Rust and TypeScript contracts.
- When applicable, regenerate the protocol output.
- Verify frontend consumers after contract changes.
- For terminal or Container CLI UI changes, verify the complete browser -> Rust API -> agent container path.
- Use Tone.js synthesis for sounds.
- Unless the product requirement explicitly changes, do not add audio files.
- Apply the [writing guide](writing.md) to UI copy and errors.
- Run the applicable checks from the [validation table](workflow.md#validation).

Cold route chunks can exceed Playwright's default 15-second action timeout.

- For this failure, wait for the route container with `waitFor({ state: 'visible' })` before the click.
- Set that click's timeout with `.click({ timeout: 30000 })`.
- Do not replace these checks with sleeps or disabled tests.
- Run Playwright MCP browser tools as a non-root user.
