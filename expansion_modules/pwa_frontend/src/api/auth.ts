// WebAuthn / Passkey client for the pwa_auth backend.

const AUTH_BASE = '/api/v1/auth';

function toBase64Url(bytes: Uint8Array<ArrayBuffer>): string {
  let bin = '';
  bytes.forEach((b) => {
    bin += String.fromCharCode(b);
  });
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function fromBase64Url(value: string): Uint8Array<ArrayBuffer> {
  const normalized = value.replace(/-/g, '+').replace(/_/g, '/');
  const pad = normalized.length % 4 === 0 ? '' : '='.repeat(4 - (normalized.length % 4));
  const bin = atob(normalized + pad);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) {
    bytes[i] = bin.charCodeAt(i);
  }
  return bytes;
}

async function post<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${AUTH_BASE}${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const errBody = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(errBody.error || `request failed: ${res.status}`);
  }
  return res.json() as Promise<T>;
}

export interface RegisterStart {
  challenge_id: string;
  challenge: string;
  rp_id: string;
  rp_name: string;
  user_id: string;
  origin: string;
  timeout_ms: number;
}

export interface RegisterFinish {
  success: boolean;
  credential_id: string;
}

export interface LoginStart {
  challenge_id: string;
  challenge: string;
  rp_id: string;
  allow_credentials: { id: string; type: string; transports: string[] }[];
  timeout_ms: number;
}

export interface LoginFinish {
  success: boolean;
  access_token: string;
  refresh_token: string;
  expires_in: number;
  session_id: string;
}

export function webauthnSupported(): boolean {
  return typeof window !== 'undefined' && !!window.PublicKeyCredential;
}

export async function registerPasskey(userId: string): Promise<RegisterFinish> {
  const start = await post<RegisterStart>('/webauthn/register/start', {
    user_id: userId,
  });

  const challenge = fromBase64Url(start.challenge);
  const userIdBytes = new TextEncoder().encode(start.user_id);

  const options: CredentialCreationOptions = {
    publicKey: {
      challenge,
      rp: { id: start.rp_id, name: start.rp_name },
      user: {
        id: userIdBytes,
        name: start.user_id,
        displayName: start.user_id,
      },
      pubKeyCredParams: [
        { type: 'public-key', alg: -7 },
        { type: 'public-key', alg: -257 },
      ],
      timeout: start.timeout_ms,
      attestation: 'none',
      authenticatorSelection: {
        residentKey: 'preferred',
        userVerification: 'preferred',
      },
    },
  };

  const credential = (await navigator.credentials.create(options)) as
    | PublicKeyCredential
    | null;
  if (!credential) {
    throw new Error('passkey creation was cancelled');
  }

  const clientDataJson = credential.response as unknown as {
    clientDataJSON: ArrayBuffer;
    attestationObject: ArrayBuffer;
  };

  const finish = await post<RegisterFinish>('/webauthn/register/finish', {
    challenge_id: start.challenge_id,
    client_data_json: toBase64Url(new Uint8Array(clientDataJson.clientDataJSON)),
    attestation_object: toBase64Url(new Uint8Array(clientDataJson.attestationObject)),
    credential_id: credential.id,
    public_key: toBase64Url(new Uint8Array(new ArrayBuffer(0))),
  });

  return finish;
}

export async function loginWithPasskey(userId: string): Promise<LoginFinish> {
  const start = await post<LoginStart>('/webauthn/login/start', {
    user_id: userId,
  });

  const challenge = fromBase64Url(start.challenge);
  const allowCredentials = start.allow_credentials.map((c) => ({
    type: 'public-key' as const,
    id: fromBase64Url(c.id),
  }));

  const options: CredentialRequestOptions = {
    publicKey: {
      challenge,
      allowCredentials,
      rpId: start.rp_id,
      timeout: start.timeout_ms,
      userVerification: 'preferred',
    },
  };

  const credential = (await navigator.credentials.get(options)) as
    | PublicKeyCredential
    | null;
  if (!credential) {
    throw new Error('passkey authentication was cancelled');
  }

  const response = credential.response as unknown as {
    authenticatorData: ArrayBuffer;
    signature: ArrayBuffer;
    clientDataJSON: ArrayBuffer;
  };

  const finish = await post<LoginFinish>('/webauthn/login/finish', {
    challenge_id: start.challenge_id,
    authenticator_data: toBase64Url(new Uint8Array(response.authenticatorData)),
    signature: toBase64Url(new Uint8Array(response.signature)),
    client_data_json: toBase64Url(new Uint8Array(response.clientDataJSON)),
  });

  return finish;
}

export async function refreshAccessToken(refreshToken: string): Promise<{
  access_token: string;
  refresh_token: string;
  expires_in: number;
}> {
  return post('/refresh', { refresh_token: refreshToken });
}

export async function logoutSession(sessionId: string): Promise<{ success: boolean }> {
  return post('/logout', { session_id: sessionId });
}
