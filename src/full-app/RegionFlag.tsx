// Flags from lipis/flag-icons (MIT), 4x3 SVGs bundled in src/assets/flags.
import eu from "../assets/flags/eu.svg";
import kr from "../assets/flags/kr.svg";
import us from "../assets/flags/us.svg";

const FLAGS: Record<string, string> = { KR: kr, EUW: eu, EUNE: eu, NA: us };

/** The flag for a League server region, or nothing for unknown regions. Decorative: the region name sits next to it. */
export function RegionFlag({ region }: { region: string }) {
  const src = FLAGS[region];
  return src ? <img className="fa-region-flag" src={src} alt="" width={20} height={15} draggable={false} /> : null;
}
