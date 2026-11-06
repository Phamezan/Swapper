import { Popover } from "radix-ui";
import { BookOpen } from "lucide-react";
import { Button } from "../components/ui/button";
import { GuideAuthorCredit } from "./MatchupGuideContent";
import type { GuideSource } from "./matchupGuide";

export function GuideSourcesPopover({ sources }: { sources: GuideSource[] }) {
  if (!sources.length) return null;
  return <Popover.Root>
    <Popover.Trigger asChild><Button type="button" variant="secondary" className="mg-sources-trigger"><BookOpen aria-hidden /><span>Sources (<b>{sources.length}</b>)</span></Button></Popover.Trigger>
    <Popover.Portal><Popover.Content className="fa-picker-popover mg-sources-popover" align="end" sideOffset={8} collisionPadding={12} aria-label="Guide sources">
      {sources.map(source => <GuideAuthorCredit key={source.id} source={source} />)}
    </Popover.Content></Popover.Portal>
  </Popover.Root>;
}
