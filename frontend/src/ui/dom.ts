export function requireElement<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (element === null) {
    throw new Error(`missing element #${id}`);
  }
  return element as T;
}

export function setText(element: HTMLElement, text: string): void {
  if (element.textContent !== text) {
    element.textContent = text;
  }
}

export function setClass(element: HTMLElement, name: string, enabled: boolean): void {
  element.classList.toggle(name, enabled);
}

export function formatNumber(value: number): string {
  if (!Number.isFinite(value)) {
    return "-";
  }
  const absolute = Math.abs(value);
  if (absolute >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(1)}M`;
  }
  if (absolute >= 10_000) {
    return `${(value / 1000).toFixed(1)}k`;
  }
  return Math.round(value).toLocaleString("en-US");
}

export function formatTick(tick: number): string {
  const minutes = Math.floor(tick / 60);
  const seconds = tick % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

export function formatRate(perMinute: number): string {
  return `${formatNumber(perMinute)}/min`;
}
