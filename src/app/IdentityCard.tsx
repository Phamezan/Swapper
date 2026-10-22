import type { DetectedIdentity } from "./types";

export function riotIdOf(identity: DetectedIdentity) {
  return `${identity.gameName}#${identity.tagLine}`;
}

export function IdentityCard({ identity }: { identity: DetectedIdentity }) {
  return (
    <div className="identity-card">
      {identity.iconDataUrl
        ? <img className="identity-icon" src={identity.iconDataUrl} alt="" />
        : <span className="identity-icon identity-avatar">{identity.gameName.slice(0, 1).toUpperCase()}</span>}
      <span className="identity-copy">
        <strong>{riotIdOf(identity)}</strong>
        {identity.region && <small>{identity.region}</small>}
      </span>
    </div>
  );
}

