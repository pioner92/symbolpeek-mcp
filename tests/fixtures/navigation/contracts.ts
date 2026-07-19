export interface Repository {
  load(id: string): Promise<string>;
}

export class MemoryRepository implements Repository {
  load(id: string): Promise<string> {
    return Promise.resolve(id);
  }
}

export class CachedRepository implements Repository {
  load(id: string): Promise<string> {
    return Promise.resolve(id);
  }
}
