# Writing And Terminology

Read this guide for instructions, documentation, UI copy, errors, and reports.

## Strict Project Profile

This project applies rules from ASD-STE100 Issue 9 to English prose.
Follow the [repository writing standards](../README.md#writing-standards) for new or changed English technical text.
The profile is mandatory for new or revised agent instructions.
Sentence-length checks do not establish full dictionary compliance.

- Use one instruction per sentence.
- Use the imperative for instructions.
- Limit instruction sentences to 20 words.
- Limit descriptive sentences to 25 words.
- Use active voice.
- Put a necessary condition before its instruction, with a comma between them.
- Use one topic per paragraph.
- Limit each paragraph to six sentences.
- Give information in the order the reader needs it.
- Keep instructions out of notes.
- Use one term for each concept.
- Use each term with one meaning.
- Use `check` and `test` only as nouns in prose.
- Preserve technical names, code identifiers, commands, paths, and protocol fields exactly.
- Before a full compliance claim, examine the official rules and dictionary.

The project applies the single-instruction rule without an exception for simultaneous actions.
Code blocks and table fragments retain their technical syntax.
These formats do not justify long prose or multiple instructions in one sentence.
For other languages, retain the same clarity, sequence, and terminology.

## Operator Instructions

- Start with the shortest safe path for a first-time operator.
- State prerequisites before commands or configuration.
- Provide copyable examples with explicit placeholders.
- State the expected result.
- State the next action.
- Put advanced details in validation, troubleshooting, or architecture sections.
- Follow [CLI platform support](../guides/cli-platform-support.md) for Linux, macOS, and Windows.

Example:

> Before first local Compose use, run `make setup`.
> Setup creates required networks and configures OAuth mount permissions.
> Run `make dev` to start the backend.
> In a separate terminal, run `npm run dev` to start the browser app.

## Project Terms

Use the [glossary](../architecture/glossary.md) for UI copy, API fields, and database concepts.
Use `Agent` instead of `Session` for the managed work actor in product text.

| Term | Meaning |
| --- | --- |
| Agent | Managed AI actor, backed by a container or provider |
| Participant | A2A orchestration participant |
| Container CLI | Task-execution CLI inside an agent container, such as `claude`, `codex`, `gemini`, or `opencode` |
| Platform CLI | Rust operator binary, `agentforge` |

`cliSessionId`, `cli_session_id`, `BaseEvent.sessionId`, `session_start`, and `session_end` are external CLI or hook protocol names.
Do not rename these protocol identifiers for vocabulary consistency.

## Review And Sources

- Review sentence length, action count, voice, conditions, paragraph scope, and terminology before committing instructions.
- Verify commands and links against current source files.
- Make sure that shortened text retains conditions, exceptions, ownership, and required validation.
- Keep one canonical rule location where possible.
- Use links instead of duplicate command catalogs or repository maps.

The [official STE site](https://www.asd-ste100.org/) identifies Issue 9, dated January 15, 2025.
The [standard](https://www.asd-ste100.org/assets/files/ASD-STE100_ISSUE9.pdf) defines procedural rules in section 5 and descriptive rules in section 6.
The [STE overview](https://www.asd-ste100.org/about_STE.html) explains controlled vocabulary and technical names.
The [Karpathy post](https://x.com/karpathy/status/2105819303471976479) motivated this writing approach.
