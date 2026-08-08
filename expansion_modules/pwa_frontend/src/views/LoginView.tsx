import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  registerPasskey,
  loginWithPasskey,
  webauthnSupported,
} from '../api/auth';
import { fromLoginFinish, setSession } from '../stores/auth';

export default function LoginView() {
  const [userId, setUserId] = useState('');
  const [mode, setMode] = useState<'login' | 'register'>('login');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const navigate = useNavigate();

  const supported = webauthnSupported();

  const handleAuth = async () => {
    if (!userId.trim()) {
      setError('enter a user id');
      return;
    }
    setBusy(true);
    setError(null);
    try {
      if (mode === 'login') {
        const finish = await loginWithPasskey(userId.trim());
        setSession(fromLoginFinish(userId.trim(), finish));
        navigate('/', { replace: true });
      } else {
        await registerPasskey(userId.trim());
        setMode('login');
        setError('Passkey registered — now sign in');
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      style={{
        minHeight: '100vh',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: '#0b0e17',
      }}
    >
      <div style={{ textAlign: 'center', padding: 40, maxWidth: 360, width: '100%' }}>
        <h1 style={{ fontSize: 34, color: '#00d4aa', marginBottom: 4 }}>THE-BRIDGE</h1>
        <p style={{ color: '#777', margin: '0 0 28px' }}>Passkey access · AI CEO · Fiat off-ramp</p>

        <input
          type="text"
          value={userId}
          onChange={(e) => setUserId(e.target.value)}
          placeholder="user id"
          style={{
            width: '100%',
            padding: 12,
            marginBottom: 12,
            background: '#141a2b',
            color: '#fff',
            border: '1px solid #22304d',
            borderRadius: 8,
          }}
        />

        <button
          onClick={handleAuth}
          disabled={busy || !supported}
          style={{
            width: '100%',
            padding: 14,
            background: '#00d4aa',
            color: '#000',
            border: 'none',
            borderRadius: 8,
            fontWeight: 'bold',
            fontSize: 15,
          }}
        >
          {busy
            ? 'Waiting for authenticator…'
            : mode === 'login'
              ? '🔑 Sign in with Passkey'
              : '🔐 Register Passkey'}
        </button>

        <button
          onClick={() => setMode((m) => (m === 'login' ? 'register' : 'login'))}
          disabled={busy}
          style={{
            width: '100%',
            padding: 12,
            marginTop: 8,
            background: 'transparent',
            color: '#00d4aa',
            border: '1px solid #00d4aa55',
            borderRadius: 8,
          }}
        >
          {mode === 'login' ? 'No passkey yet? Register one' : 'Already registered? Sign in'}
        </button>

        {!supported && (
          <p style={{ color: '#e94560', fontSize: 12, marginTop: 12 }}>
            WebAuthn is not supported in this browser/context (requires a secure origin).
          </p>
        )}
        {error && (
          <p style={{ color: '#e94560', fontSize: 13, marginTop: 12, wordBreak: 'break-word' }}>
            {error}
          </p>
        )}
      </div>
    </div>
  );
}
