export function formatTimestamp(d: Date): string {
  return d.toISOString().slice(0, 10);
}

export function formatDate(d: Date): string {
  return `${d.getDate()}/${d.getMonth() + 1}/${d.getFullYear()}`;
}

// This export is silenced by delve:used
/* delve:used */
export function oldHelper(): void {
  const x = 1;
}

function privateHelper(): void {
  const x = 1;
}
