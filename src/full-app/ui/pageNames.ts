/** Short display names for pages whose route keys stay unchanged. */
const SHORT: Record<string, string> = { "Tier list & Builds": "Tierlist", "Champion select": "Draft", "Live Game": "Live game" };

export const pageLabel = (page: string): string => SHORT[page] ?? page;
