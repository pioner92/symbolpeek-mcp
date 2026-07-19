function sealed(target: Function) { return target; }

export type Identifier = string | number;
export type Pair<T> = { left: T } & { right: T };

export interface Repository<T> {
  get(id: Identifier): Promise<T>;
  get(id: Identifier, cache: boolean): Promise<T>;
}

export enum Status {
  Ready,
  Failed,
}

export namespace Domain {
  export const version = 1;
}

@sealed
export abstract class BaseRepository {
  protected abstract load(): Promise<unknown>;
  private cache = new Map<Identifier, unknown>();
  static create() { return new BaseRepository(); }
  get ready() { return true; }
  set ready(value: boolean) { void value; }
}

export function load(id: Identifier): Promise<unknown>;
export function load(id: Identifier, cache: boolean): Promise<unknown>;
export async function load(id: Identifier, cache = true) {
  return { id, cache };
}

export const values = (items: Identifier[]) => items.map(String);
export function* stream() { yield* []; }

