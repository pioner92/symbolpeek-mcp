import type { User } from "./types";

export function useAuth(user: User): boolean {
  return Boolean(user.id);
}
