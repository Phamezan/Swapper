import { lazy, Suspense, useEffect, useState, type ComponentProps } from "react";
import type { RunesPanel } from "../runes/RunesPanel";
import { ChampionSelect } from "./ChampionSelect";
import { useInGame } from "./LiveGameData";
import { draftStage } from "./LiveGameModel";
import { Skeleton } from "./ui/States";
import { PageHeader } from "./ui/PageHeader";

const LiveGame = lazy(() => import("./LiveGame"));

export function DraftPage(props: ComponentProps<typeof RunesPanel>) {
  const inGame = useInGame();
  const [names, setNames] = useState(new Map<number, string>());
  useEffect(() => {
    let live = true;
    void props.championApi.list().then(list => { if (live) setNames(new Map(list.map(c => [c.id, c.name]))); }).catch(() => {});
    return () => { live = false; };
  }, []);
  return draftStage(props.view?.phase, inGame) === "live"
    ? <><PageHeader title="Draft" /><Suspense fallback={<Skeleton label="Loading game" rows={5} height={56} />}><LiveGame championNames={names} /></Suspense></>
    : <ChampionSelect {...props} />;
}
