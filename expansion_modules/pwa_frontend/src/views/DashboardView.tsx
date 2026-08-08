import { useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { getSession, clearSession } from '../stores/auth';
import {
  fetchAiHealth,
  fetchSnapshot,
  createDeposit,
  createWithdrawal,
  provisionCard,
} from '../api/services';

interface Snapshot {
  timestamp: string;
  ollama_available: boolean;
  current_mode: string;
  liquidity_score: number;
  active_breaches: number;
}

export default function DashboardView() {
  const navigate = useNavigate();
  const session = getSession()!;
  const [health, setHealth] = useState<{ ollama_available: boolean } | null>(null);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [tab, setTab] = useState<'overview' | 'fiat'>('overview');
  const [amount, setAmount] = useState('');
  const [currency, setCurrency] = useState('USD');
  const [chainId, setChainId] = useState('ethereum');
  const [result, setResult] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setHealth(await fetchAiHealth(session));
      setSnapshot((await fetchSnapshot(session)) as Snapshot);
    } catch {
      // AI bridge may be offline; keep dashboard functional
    }
  }, [session]);

  useEffect(() => {
    void load();
  }, [load]);

  const logout = () => {
    clearSession();
    navigate('/login', { replace: true });
  };

  const doDeposit = async () => {
    setBusy(true);
    setResult(null);
    try {
      const res = await createDeposit(session, {
        user_id: session.user_id,
        amount,
        currency,
        method: 'banxa',
        destination_wallet: '0x' + '0'.repeat(40),
        chain_id: chainId,
      });
      setResult(JSON.stringify(res));
    } catch (e) {
      setResult(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const doWithdraw = async () => {
    setBusy(true);
    setResult(null);
    try {
      const res = await createWithdrawal(session, {
        user_id: session.user_id,
        amount,
        currency,
        method: 'stripe',
        source_wallet: '0x' + '0'.repeat(40),
        chain_id: chainId,
        idempotency_key: crypto.randomUUID(),
      });
      setResult(JSON.stringify(res));
    } catch (e) {
      setResult(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const doCard = async () => {
    setBusy(true);
    setResult(null);
    try {
      const res = await provisionCard(session, {
        user_id: session.user_id,
        wallet_address: '0x' + '0'.repeat(40),
        chain_id: chainId,
        initial_load: amount,
        currency,
        contactless: true,
        idempotency_key: crypto.randomUUID(),
      });
      setResult(JSON.stringify(res));
    } catch (e) {
      setResult(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div style={{ minHeight: '100vh', background: '#0b0e17' }}>
      <nav
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          padding: '14px 24px',
          background: '#141a2b',
          borderBottom: '1px solid #22304d',
        }}
      >
        <h1 style={{ margin: 0, fontSize: 20, color: '#00d4aa' }}>THE-BRIDGE</h1>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          <span style={{ color: '#777', fontSize: 13 }}>{session.user_id}</span>
          <button
            onClick={logout}
            style={{
              padding: '8px 14px',
              background: '#1c2540',
              color: '#fff',
              border: '1px solid #2a3754',
              borderRadius: 6,
            }}
          >
            Logout
          </button>
        </div>
      </nav>

      <div style={{ maxWidth: 680, margin: '0 auto', padding: 24 }}>
        <div style={{ display: 'flex', gap: 8, marginBottom: 20 }}>
          {(['overview', 'fiat'] as const).map((t) => (
            <button
              key={t}
              onClick={() => setTab(t)}
              style={{
                padding: '10px 20px',
                background: tab === t ? '#00d4aa' : '#141a2b',
                color: '#fff',
                border: 'none',
                borderRadius: 8,
                fontWeight: tab === t ? 'bold' : 'normal',
              }}
            >
              {t === 'overview' ? 'AI Overview' : 'Fiat Off-Ramp'}
            </button>
          ))}
        </div>

        {tab === 'overview' && (
          <div>
            <div
              style={{
                background: '#141a2b',
                borderRadius: 12,
                padding: 20,
                marginBottom: 14,
                border: '1px solid #00d4aa33',
              }}
            >
              <h2 style={{ margin: '0 0 10px', fontSize: 16, color: '#00d4aa' }}>
                AI CEO Bridge
              </h2>
              <p style={{ margin: '4px 0', color: '#aaa', fontSize: 14 }}>
                Ollama backend:{' '}
                {health === null ? '…' : health.ollama_available ? 'available' : 'unavailable'}
              </p>
              {snapshot && (
                <>
                  <p style={{ margin: '4px 0', color: '#aaa', fontSize: 14 }}>
                    Trading mode: <b style={{ color: '#fff' }}>{snapshot.current_mode}</b>
                  </p>
                  <p style={{ margin: '4px 0', color: '#aaa', fontSize: 14 }}>
                    Liquidity score:{' '}
                    <b style={{ color: '#fff' }}>{(snapshot.liquidity_score * 100).toFixed(1)}%</b>
                  </p>
                  <p style={{ margin: '4px 0', color: '#aaa', fontSize: 14 }}>
                    Active breaches: <b style={{ color: '#fff' }}>{snapshot.active_breaches}</b>
                  </p>
                </>
              )}
            </div>
          </div>
        )}

        {tab === 'fiat' && (
          <div style={{ background: '#141a2b', borderRadius: 12, padding: 20 }}>
            <h2 style={{ margin: '0 0 14px', fontSize: 16, color: '#00d4aa' }}>
              Fiat Off-Ramp
            </h2>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
              <input
                type="text"
                value={amount}
                onChange={(e) => setAmount(e.target.value)}
                placeholder="amount"
                style={{
                  padding: 10,
                  background: '#0b0e17',
                  color: '#fff',
                  border: '1px solid #22304d',
                  borderRadius: 8,
                }}
              />
              <select
                value={currency}
                onChange={(e) => setCurrency(e.target.value)}
                style={{
                  padding: 10,
                  background: '#0b0e17',
                  color: '#fff',
                  border: '1px solid #22304d',
                  borderRadius: 8,
                }}
              >
                {['USD', 'EUR', 'EGP', 'GBP', 'AED'].map((c) => (
                  <option key={c} value={c}>
                    {c}
                  </option>
                ))}
              </select>
            </div>
            <select
              value={chainId}
              onChange={(e) => setChainId(e.target.value)}
              style={{
                width: '100%',
                padding: 10,
                marginTop: 10,
                background: '#0b0e17',
                color: '#fff',
                border: '1px solid #22304d',
                borderRadius: 8,
              }}
            >
              {['ethereum', 'polygon', 'arbitrum', 'optimism'].map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </select>

            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 10, marginTop: 14 }}>
              <button
                onClick={doDeposit}
                disabled={busy}
                style={{
                  padding: 12,
                  background: '#00d4aa',
                  color: '#000',
                  border: 'none',
                  borderRadius: 8,
                  fontWeight: 'bold',
                }}
              >
                Deposit
              </button>
              <button
                onClick={doWithdraw}
                disabled={busy}
                style={{
                  padding: 12,
                  background: '#1c2540',
                  color: '#fff',
                  border: '1px solid #2a3754',
                  borderRadius: 8,
                }}
              >
                Withdraw
              </button>
              <button
                onClick={doCard}
                disabled={busy}
                style={{
                  padding: 12,
                  background: '#1c2540',
                  color: '#fff',
                  border: '1px solid #2a3754',
                  borderRadius: 8,
                }}
              >
                Card
              </button>
            </div>

            {result && (
              <pre
                style={{
                  marginTop: 14,
                  padding: 12,
                  background: '#0b0e17',
                  border: '1px solid #22304d',
                  borderRadius: 8,
                  fontSize: 12,
                  overflowX: 'auto',
                  whiteSpace: 'pre-wrap',
                  wordBreak: 'break-word',
                  color: '#00d4aa',
                }}
              >
                {result}
              </pre>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
