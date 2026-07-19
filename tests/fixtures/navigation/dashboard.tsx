import { useAuth } from "./auth";
import type { User } from "./types";

export function Dashboard({ user }: { user: User }) {
  return <section>{useAuth(user) ? "signed in" : "signed out"}</section>;
}
