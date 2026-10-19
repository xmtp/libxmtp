// One owner per Hermes generation, shared by a headless task and its later UI.
let claimed = false;
export function claimLaunch(url: string): boolean {
  if (claimed || !url.startsWith("xmtprn://run?")) return false;
  claimed = true;
  return true;
}
