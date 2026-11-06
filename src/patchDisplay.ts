/** Riot public patch names; keep the original version for requests and cache keys. */
export function displayPatch(version: string): string {
  const match = /^(\d+)\.(\d+)(?:\.\d+)*$/.exec(version);
  if (!match) return version;
  const major = Number(match[1]);
  return `${major >= 15 && major <= 19 ? major + 10 : match[1]}.${match[2]}`;
}
