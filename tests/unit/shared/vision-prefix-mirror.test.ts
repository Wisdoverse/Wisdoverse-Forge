import fs from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

// The OpenAI vision allowlist (`OPENAI_VISION_PREFIXES`) is hand-maintained in two
// languages: the Rust server gate `agentforge_llm::vision` is the source of truth,
// and the browser gate `@app/entities/agent` mirrors it so the UI never advertises
// (or hides) an image-upload affordance the backend's model-aware gate disagrees
// with. The two lists silently drifting is exactly the bug class fixed in #971/#973;
// this guard fails CI the moment they diverge so the mirror stays honest.

const projectRoot = path.resolve(import.meta.dirname, '../../..')
const rustPath = path.join(projectRoot, 'rust/crates/llm/src/vision.rs')
const tsPath = path.join(projectRoot, 'src/app/entities/agent/model/runtime-kind.ts')

/** Drop `//` line comments so their contents (which contain commas) never leak into
 * literal collection. Model-name prefixes contain no `//`, so this is safe here. */
function stripLineComments(body: string): string {
  return body
    .split('\n')
    .map((line) => line.replace(/\/\/.*$/, ''))
    .join('\n')
}

/** Extract the Rust `const OPENAI_VISION_PREFIXES: &[&str] = &[ ... ];` literals. */
function extractRustPrefixes(src: string): string[] {
  const m = src.match(/const OPENAI_VISION_PREFIXES\s*:[^=]*=\s*&\[([\s\S]*?)\];/)
  if (!m) {
    throw new Error(
      `Could not find \`const OPENAI_VISION_PREFIXES: &[&str] = &[...]\` in ${rustPath}. ` +
        `If it was renamed or reformatted, update this guard.`
    )
  }
  return Array.from(stripLineComments(m[1]).matchAll(/"([^"]*)"/g), (g) => g[1])
}

/** Extract the TS `const OPENAI_VISION_PREFIXES = [ ... ]` literals (single or double quoted). */
function extractTsPrefixes(src: string): string[] {
  const m = src.match(/const OPENAI_VISION_PREFIXES\s*=\s*\[([\s\S]*?)\]/)
  if (!m) {
    throw new Error(
      `Could not find \`const OPENAI_VISION_PREFIXES = [...]\` in ${tsPath}. ` +
        `If it was renamed or reformatted, update this guard.`
    )
  }
  return Array.from(stripLineComments(m[1]).matchAll(/['"]([^'"]*)['"]/g), (g) => g[1])
}

describe('OPENAI_VISION_PREFIXES stays mirrored across Rust (source of truth) and TS', () => {
  it('the TS UI mirror matches the Rust server gate exactly', () => {
    const rust = extractRustPrefixes(fs.readFileSync(rustPath, 'utf8'))
    const ts = extractTsPrefixes(fs.readFileSync(tsPath, 'utf8'))

    // Non-empty guards against a regex silently yielding [] (which would make the
    // equality below a false-green of [] === []).
    expect(rust.length, `no prefixes extracted from ${rustPath}`).toBeGreaterThan(0)
    expect(ts.length, `no prefixes extracted from ${tsPath}`).toBeGreaterThan(0)

    expect(
      ts,
      `OPENAI_VISION_PREFIXES drift: TS mirror ${tsPath} must equal Rust source of truth ${rustPath}`
    ).toEqual(rust)
  })

  it('the extractors actually detect drift (not just that today files happen to match)', () => {
    const rust = extractRustPrefixes(
      'const OPENAI_VISION_PREFIXES: &[&str] = &["gpt-4o", "gpt-5"];'
    )
    const divergentTs = extractTsPrefixes("const OPENAI_VISION_PREFIXES = ['gpt-4o']") // drops gpt-5
    expect(rust).toEqual(['gpt-4o', 'gpt-5'])
    expect(divergentTs).not.toEqual(rust)
  })

  it('throws a clear error if either declaration is renamed or removed', () => {
    expect(() => extractRustPrefixes('const SOMETHING_ELSE = &["x"];')).toThrow(
      /OPENAI_VISION_PREFIXES/
    )
    expect(() => extractTsPrefixes('const SOMETHING_ELSE = ["x"]')).toThrow(
      /OPENAI_VISION_PREFIXES/
    )
  })
})
