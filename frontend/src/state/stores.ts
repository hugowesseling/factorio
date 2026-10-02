export type Listener<T> = (value: T, previous: T) => void;

export class Store<T> {
  private listeners = new Set<Listener<T>>();
  private current: T;

  constructor(initial: T) {
    this.current = initial;
  }

  get value(): T {
    return this.current;
  }

  set(next: T): void {
    if (Object.is(next, this.current)) {
      return;
    }
    const previous = this.current;
    this.current = next;
    for (const listener of [...this.listeners]) {
      listener(next, previous);
    }
  }

  update(mutate: (current: T) => T): void {
    this.set(mutate(this.current));
  }

  subscribe(listener: Listener<T>): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  get listenerCount(): number {
    return this.listeners.size;
  }
}
