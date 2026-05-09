export interface KeyDef {
  label: string
  keys: string[]
  className?: string
}

export const NAV_KEYS: KeyDef[] = [
  { label: '\u2191', keys: ['\x1b[A'] },
  { label: '\u2193', keys: ['\x1b[B'] },
  { label: '\u2190', keys: ['\x1b[D'] },
  { label: '\u2192', keys: ['\x1b[C'] },
  { label: 'Enter', keys: ['\r'], className: 'key-wide' },
  { label: 'Esc', keys: ['\x1b'], className: 'key-wide' },
  { label: 'Ctrl+C', keys: ['\x03'], className: 'key-wide key-danger' },
]

export const NUM_KEYS: KeyDef[] = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'].map((n) => ({
  label: n,
  keys: [n],
}))

export const UTIL_KEYS: KeyDef[] = [
  { label: 'Tab', keys: ['\t'], className: 'key-wide' },
  { label: 'Space', keys: [' '], className: 'key-wide' },
  { label: '\u232b', keys: ['\x7f'] },
  { label: 'y', keys: ['y'] },
  { label: 'n', keys: ['n'] },
]
