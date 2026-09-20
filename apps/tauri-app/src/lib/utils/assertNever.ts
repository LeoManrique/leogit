/**
 * The `default:` of a `switch` that is meant to cover every member of a union.
 *
 * TypeScript narrows the switched value to `never` once every case is handled,
 * so passing it here compiles only while that stays true: a variant core adds
 * later turns each such `switch` into a `pnpm check` error instead of a button
 * that silently does nothing. At run time — a payload from a newer core than
 * this bundle was built against — it says which value nobody handled.
 */
export function assertNever(unhandled: never): never {
  throw new Error(`Unhandled variant: ${JSON.stringify(unhandled)}`)
}
