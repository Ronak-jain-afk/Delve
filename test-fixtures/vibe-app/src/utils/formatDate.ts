export function formatTimestamp(d: Date): string {
  return d.toISOString().slice(0, 10);
}

export function formatDate(d: Date): string {
  return `${d.getDate()}/${d.getMonth() + 1}/${d.getFullYear()}`;
}

function oldHelper(): void {
  /* delve:used */
  const x = 1;
}
