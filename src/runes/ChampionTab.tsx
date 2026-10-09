import { useEffect, useMemo, useRef, useState } from "react";
import { ChampionIcon } from "./ChampionIcon";
import { CachedBadge } from "./CachedBadge";
import { RoleIcon } from "./RoleIcon";
import { CardStrip } from "./CounterStrips";
import { ChampionOverview } from "./ChampionOverview";
import { RankPicker } from "./RankPicker";
import { TierList } from "./TierList";
import { TIER_OPTIONS } from "./types";
import type {
  ChampionApi,
  ChampionCountersView,
  ChampionOption,
  ChampionOverviewView,
  MatchupView,
} from "./types";

const LANES = [
  { value: "top", label: "Top" },
  { value: "jungle", label: "Jungle" },
  { value: "mid", label: "Mid" },
  { value: "adc", label: "Bot" },
  { value: "support", label: "Support" },
] as const;
/** Search suggestions shown at once. */
const MAX_SUGGESTIONS = 8;
type Remembered = {
  session: string;
  champion: ChampionOption | null;
  query: string;
  lane: string | null;
  tier: string | null;
  /** The prefill last applied, so only a new pick or enemy replaces a search. */
  prefillId: number;
};

const NOTHING: Remembered = { session: "", champion: null, query: "", lane: null, tier: null, prefillId: 0 };

/** What the user last searched, kept so switching tabs (which unmounts this
 *  one) does not lose it. It belongs to one champion-select session. */
let remembered: Remembered = NOTHING;

function recall(session: string): Remembered {
  if (remembered.session !== session) remembered = { ...NOTHING, session };
  return remembered;
}

/** The lane the tier list starts on when nothing says otherwise. */
const DEFAULT_LANE = "mid";

type Props = {
  mode: "desktop" | "remote";
  /** The rank bracket to start on; the tab keeps its own after that. */
  initialTier: string;
  /** The enemy picked in champion select, searched on open. */
  prefillChampion: ChampionOption | null;
  /** Our own role in champion select, or "" when unknown. */
  prefillPosition: string;
  /** Changes when a new champion-select session starts, dropping the search. */
  sessionKey: string;
  api: ChampionApi;
  onLoadMatchup: (
    championId: number,
    enemyChampionId: number,
    position: string,
    tier: string,
  ) => Promise<MatchupView | null>;
};

/** The Champion page: a lane tier list until a champion is searched, then its
 *  overview, build and matchups. Nothing is applied from here. */
export function ChampionTab({
  mode,
  initialTier,
  prefillChampion,
  prefillPosition,
  sessionKey,
  api,
  onLoadMatchup,
}: Props) {
  const [champions, setChampions] = useState<ChampionOption[]>([]);
  // A new prefill (the picked champion or enemy) wins over what was remembered;
  // a prefill already applied earlier in this session does not.
  const [start] = useState(() => {
    const saved = recall(sessionKey);
    if (prefillChampion && prefillChampion.id !== saved.prefillId) {
      remembered.prefillId = prefillChampion.id;
      return {
        champion: prefillChampion,
        query: prefillChampion.name,
        lane: prefillPosition || null,
        tier: saved.tier ?? initialTier,
      };
    }
    return { ...saved, tier: saved.tier ?? initialTier };
  });
  const [query, setQuery] = useState(start.query);
  const [champion, setChampion] = useState<ChampionOption | null>(start.champion);
  const [lane, setLane] = useState<string | null>(start.lane);
  const [tier, setTier] = useState(start.tier);
  const [data, setData] = useState<ChampionCountersView | null>(null);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  const [overview, setOverview] = useState<ChampionOverviewView | null>(null);

  useEffect(() => {
    let live = true;
    api.list()
      .then((list) => {
        if (live) setChampions(list);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
    // Loaded once when the tab opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // A prefill replaces the search only when the picked champion or enemy is
  // new; otherwise what the user typed stays.
  const prefillId = prefillChampion?.id ?? 0;
  useEffect(() => {
    recall(sessionKey);
    if (!prefillChampion || prefillId === remembered.prefillId) return;
    remembered.prefillId = prefillId;
    setChampion(prefillChampion);
    setQuery(prefillChampion.name);
    if (prefillPosition) setLane(prefillPosition);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prefillId]);

  useEffect(() => {
    remembered = { ...recall(sessionKey), champion, query, lane, tier };
  }, [champion, query, lane, tier, sessionKey]);

  // A new champion-select session starts from a clean search.
  const firstSession = useRef(sessionKey);
  useEffect(() => {
    if (firstSession.current === sessionKey) return;
    firstSession.current = sessionKey;
    recall(sessionKey);
    setChampion(null);
    setQuery("");
    setLane(prefillPosition || null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionKey]);

  const championId = champion?.id ?? 0;
  useEffect(() => {
    setData(null);
    setOverview(null);
    setFailed(false);
    if (championId <= 0) return;
    let live = true;
    setLoading(true);
    api
      .overview(championId, lane, tier)
      .then((next) => {
        if (live) setOverview(next);
      })
      .catch(() => {});
    api
      .counters(championId, lane, tier)
      .then((next) => {
        if (live) setData(next);
      })
      .catch(() => {
        if (live) setFailed(true);
      })
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
    // The loader is a stable wrapper; only the filter values should refetch.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [championId, lane, tier]);

  const suggestions = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle || champion?.name.toLowerCase() === needle) return [];
    return champions
      .filter((option) => option.name.toLowerCase().includes(needle))
      .slice(0, MAX_SUGGESTIONS);
  }, [query, champions, champion]);

  const listLane = lane ?? (prefillPosition || DEFAULT_LANE);
  const activeLane = championId > 0 ? (lane ?? data?.position ?? "") : listLane;
  // Matchup lookups need a concrete lane even before the page reports one.
  const matchupLane = activeLane || listLane;
  const unavailable = failed || data?.unavailable === true;

  return (
    <div className="champion-tab">
      <div className="champion-filter">
        <RankPicker tiers={TIER_OPTIONS} value={tier} games={0} disabled={false} mode={mode} onChange={setTier} />
      </div>
      <div className="champion-search">
        <input
          type="search"
          value={query}
          placeholder="Search a champion"
          aria-label="Search a champion"
          autoComplete="off"
          onChange={(event) => {
            setQuery(event.target.value);
            // Typing something else drops the champion (and its overview and
            // cards); the tier list returns until a new one is picked.
            if (event.target.value.trim().toLowerCase() !== champion?.name.toLowerCase()) {
              setChampion(null);
            }
          }}
        />
        {suggestions.length > 0 && (
          <ul className="champion-suggestions">
            {suggestions.map((option) => (
              <li key={option.id}>
                <button
                  type="button"
                  onClick={() => {
                    setChampion(option);
                    setQuery(option.name);
                    setLane(prefillPosition || null);
                  }}
                >
                  <ChampionIcon id={option.id} mode={mode} />
                  {option.name}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="runes-role-picker" role="group" aria-label="Choose lane">
        {LANES.map((option) => (
          <button
            key={option.value}
            type="button"
            className={activeLane === option.value ? "is-active" : ""}
            aria-pressed={activeLane === option.value}
            aria-label={option.label}
            title={option.label}
            onClick={() => setLane(option.value)}
          >
            <RoleIcon role={option.value} size={17} mode={mode} />
          </button>
        ))}
      </div>

      {championId <= 0 ? (
        <TierList
          mode={mode}
          position={listLane}
          tier={tier}
          onLoad={api.tierList}
          onPick={(picked) => {
            setChampion(picked);
            setQuery(picked.name);
            setLane(listLane);
          }}
        />
      ) : (
        <>
          <ChampionOverview mode={mode} championId={championId} name={champion?.name ?? ""} view={overview} loading={loading} />
          {data?.stale && <CachedBadge stale updatedAt={data.updatedAt} />}
          {unavailable && !loading && (
            <p className="runes-note">Matchup data for {champion?.name} is unavailable right now.</p>
          )}
          <CardStrip
            title="Toughest matchups"
            subtitle={`These champions counter ${champion?.name ?? ""}`}
            tone="loss"
            rows={data?.bestInto}
            loading={loading}
            {...{ mode, tier, championId, lane: matchupLane, onLoadMatchup }}
          />
          <CardStrip
            title="Easiest matchups"
            subtitle={`${champion?.name ?? ""} counters these champions`}
            tone="win"
            rows={data?.beats}
            loading={loading}
            {...{ mode, tier, championId, lane: matchupLane, onLoadMatchup }}
          />
        </>
      )}
    </div>
  );
}
