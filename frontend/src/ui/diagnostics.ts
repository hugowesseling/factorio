export type DiagnosticLevel = "info" | "ok" | "warn" | "error";

export interface DiagnosticsConsole {
  log(level: DiagnosticLevel, text: string): void;
  state(text: string): void;
  ready(): void;
  toggle(): void;
  clear(): void;
}

declare global {
  interface Window {
    __factorioConsole?: DiagnosticsConsole;
  }
}

export function diag(level: DiagnosticLevel, text: string): void {
  const bus = typeof window === "undefined" ? undefined : window.__factorioConsole;
  if (bus !== undefined) {
    bus.log(level, text);
    return;
  }
  const method = level === "ok" ? "log" : level;
  console[method](`[factorio] ${text}`);
}

export function diagState(text: string): void {
  window.__factorioConsole?.state(text);
}

export function diagReady(): void {
  window.__factorioConsole?.ready();
}

export function describeError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
