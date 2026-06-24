#!/usr/bin/env node
const command = process.argv[2] ?? 'command'
console.error(`${command} is not supported after the Rust migration cutover.`)
console.error('Use PostgreSQL backups for rollback and inspect _sqlx_migrations for status.')
process.exit(1)
