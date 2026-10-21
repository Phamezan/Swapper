/** The input is chronological. Flat samples sit in the centre of the viewbox. */
export function sparkline(points: number[]): string {
  const min = Math.min(...points);
  const range = Math.max(...points) - min;
  return points.map((n, i) => `${points.length === 1 ? 50 : i * 100 / (points.length - 1)},${range ? 24 - (n - min) / range * 20 : 14}`).join(" ");
}

/** The area under `sparkline(points)`, closed along the bottom of a 0 0 100 28 viewbox. */
export function sparkArea(points: number[]): string {
  const line = sparkline(points);
  if (!line) return "";
  const xs = line.split(" ").map(pair => pair.split(",")[0]);
  return `M${line.split(" ").join(" L")} L${xs[xs.length - 1]},28 L${xs[0]},28 Z`;
}
