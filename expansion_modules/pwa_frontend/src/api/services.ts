// API clients for AI CEO bridge and fiat off-ramp.

import { Session } from '../stores/auth';

async function authedJson<T>(path: string, session: Session, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${session.access_token}`,
      ...(init?.headers ?? {}),
    },
  });
  if (!res.ok) {
    throw new Error(`request failed: ${res.status}`);
  }
  return res.json() as Promise<T>;
}

export interface HealthResponse {
  ok: boolean;
  service: string;
  ollama_available: boolean;
  uptime_seconds: number;
}

export async function fetchAiHealth(session: Session): Promise<HealthResponse> {
  return authedJson<HealthResponse>('/api/v1/ai/health', session);
}

export async function fetchSnapshot(session: Session) {
  return authedJson('/api/v1/ai/snapshot', session);
}

export interface DepositPayload {
  user_id: string;
  amount: string;
  currency: string;
  method: string;
  destination_wallet: string;
  chain_id: string;
}

export interface WithdrawPayload {
  user_id: string;
  amount: string;
  currency: string;
  method: string;
  source_wallet: string;
  chain_id: string;
  idempotency_key: string;
}

export async function createDeposit(session: Session, payload: DepositPayload) {
  return authedJson('/api/v1/fiat/deposit', session, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export async function createWithdrawal(session: Session, payload: WithdrawPayload) {
  return authedJson('/api/v1/fiat/withdraw', session, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export async function provisionCard(
  session: Session,
  payload: {
    user_id: string;
    wallet_address: string;
    chain_id: string;
    initial_load: string;
    currency: string;
    contactless: boolean;
    idempotency_key: string;
  },
) {
  return authedJson('/api/v1/fiat/card/provision', session, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}
