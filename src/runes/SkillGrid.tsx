/** The ability rows of a skill-order grid, in display order. */
const SKILL_ROWS = ["Q", "W", "E", "R"] as const;

/** Q/W/E/R rows by level columns; the filled cell shows its level. */
export function SkillGrid({ order }: { order: string[] }) {
  return (
    <div className="pro-skill-rows" aria-label={`Skill order: ${order.join(" ")}`}>
      {SKILL_ROWS.map((letter) => (
        <div key={letter} className="pro-skill-row">
          <span className="pro-skill-label">{letter}</span>
          {order.map((chosen, level) => (
            <span key={level} className={`pro-skill-cell ${chosen === letter ? "is-filled" : ""}`}>
              {chosen === letter ? level + 1 : ""}
            </span>
          ))}
        </div>
      ))}
    </div>
  );
}
