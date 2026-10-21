/** The input is chronological. Flat samples sit in the centre of the viewbox. */
export function sparkline(points: number[]): string {
  const min = Math.min(...points);
  const range = Math.max(...points) - min;
  return points.map((n, i) => `${points.length === 1 ? 50 : i * 100 / (points.length - 1)},${range ? 24 - (n - min) / range * 20 : 14}`).join(" ");
}
