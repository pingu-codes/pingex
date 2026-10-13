/** The CLI version embedded in a `userAgent`, or null when it cannot be read. */
export function codexVersionFromUserAgent(userAgent: string | null | undefined): string | null {
  const match = userAgent?.match(/^[\w.-]+\/(\d+\.\d+\.\d+(?:-[\w.]+)?)/);
  return match?.[1] ?? null;
}
