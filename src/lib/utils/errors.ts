/** The human-readable message of a thrown value, whatever was thrown. */
export function errorMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}
