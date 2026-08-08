import { create } from './storage';
import { LoginFinish } from '../api/auth';

export interface Session {
  access_token: string;
  refresh_token: string;
  expires_in: number;
  user_id: string;
}

const SESSION_KEY = 'thebridge.session';

export const sessionStore = create<Session | null>(SESSION_KEY);

export function getSession(): Session | null {
  return sessionStore.get();
}

export function setSession(session: Session): void {
  sessionStore.set(session);
}

export function clearSession(): void {
  sessionStore.remove();
}

export function fromLoginFinish(userId: string, finish: LoginFinish): Session {
  return {
    access_token: finish.access_token,
    refresh_token: finish.refresh_token,
    expires_in: finish.expires_in,
    user_id: userId,
  };
}
