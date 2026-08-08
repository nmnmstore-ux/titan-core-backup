// ============================================================
// SwiftBridge Web App - Chain Abstraction UI
// Sovereign Master Prompt: تخفي تعقيدات البلوكشين تماماً
// Smart Accounts + Biometrics + Web2 Experience
// ============================================================

import React, { useState, useEffect } from 'react';

// Biometric + Passkey types
type AuthMethod = 'none' | 'biometric' | 'passkey' | 'pin';
interface SmartAccount {
    address: string;
    biometricRegistered: boolean;
    passkeyRegistered: boolean;
    tss: boolean; // Threshold signature scheme
}

type Currency = { code: string; name: string; flag: string };
type TransferStep = 1 | 2 | 3;

const CURRENCIES: Currency[] = [
    { code: 'USD', name: 'US Dollar', flag: '🇺🇸' }, { code: 'EUR', name: 'Euro', flag: '🇪🇺' },
    { code: 'EGP', name: 'Egyptian Pound', flag: '🇪🇬' }, { code: 'SAR', name: 'Saudi Riyal', flag: '🇸🇦' },
    { code: 'AED', name: 'UAE Dirham', flag: '🇦🇪' }, { code: 'GBP', name: 'British Pound', flag: '🇬🇧' },
    { code: 'PKR', name: 'Pakistani Rupee', flag: '🇵🇰' }, { code: 'INR', name: 'Indian Rupee', flag: '🇮🇳' },
    { code: 'TRY', name: 'Turkish Lira', flag: '🇹🇷' }, { code: 'NGN', name: 'Nigerian Naira', flag: '🇳🇬' },
    { code: 'JPY', name: 'Japanese Yen', flag: '🇯🇵' }, { code: 'CNY', name: 'Chinese Yuan', flag: '🇨🇳' },
    { code: 'CHF', name: 'Swiss Franc', flag: '🇨🇭' }, { code: 'AUD', name: 'Australian Dollar', flag: '🇦🇺' },
    { code: 'CAD', name: 'Canadian Dollar', flag: '🇨🇦' }, { code: 'MXN', name: 'Mexican Peso', flag: '🇲🇽' },
    { code: 'KES', name: 'Kenyan Shilling', flag: '🇰🇪' }, { code: 'ZAR', name: 'South African Rand', flag: '🇿🇦' },
    { code: 'MAD', name: 'Moroccan Dirham', flag: '🇲🇦' }, { code: 'ILS', name: 'Israeli Shekel', flag: '🇮🇱' },
];

const App: React.FC = () => {
    const [loggedIn, setLoggedIn] = useState(false);
    const [authMethod, setAuthMethod] = useState<AuthMethod>('none');
    const [smartAccount] = useState<SmartAccount>({
        address: '0xSWB_' + Math.random().toString(36).substr(2, 12),
        biometricRegistered: true,
        passkeyRegistered: true,
        tss: true,
    });
    const [page, setPage] = useState<'transfer' | 'dashboard' | 'dao'>('transfer');

    // Simulate biometric/passkey auth
    const handleBiometricLogin = async () => {
        setAuthMethod('biometric');
        await new Promise(r => setTimeout(r, 800));
        setLoggedIn(true);
    };

    const handlePasskeyLogin = async () => {
        setAuthMethod('passkey');
        await new Promise(r => setTimeout(r, 1000));
        setLoggedIn(true);
    };

    if (!loggedIn) {
        return (
            <div style={{ fontFamily: 'Arial', background: '#0a0a1a', minHeight: '100vh', display: 'flex', alignItems: 'center', justifyContent: 'center', color: '#fff' }}>
                <div style={{ textAlign: 'center', padding: 40 }}>
                    <h1 style={{ fontSize: 36, color: '#00d4aa', marginBottom: 8 }}>SwiftBridge</h1>
                    <p style={{ color: '#888', marginBottom: 32 }}>Smart Account • Self-Sovereign • zk-Private</p>
                    <button onClick={handleBiometricLogin}
                        style={{ display: 'block', width: 280, padding: 16, margin: '8px auto', background: '#00d4aa', color: '#000', border: 'none', borderRadius: 10, fontSize: 16, fontWeight: 'bold', cursor: 'pointer' }}>
                        🔐 Sign with Biometric
                    </button>
                    <button onClick={handlePasskeyLogin}
                        style={{ display: 'block', width: 280, padding: 16, margin: '8px auto', background: '#141428', color: '#fff', border: '1px solid #2a2a4a', borderRadius: 10, fontSize: 16, cursor: 'pointer' }}>
                        🔑 Sign with Passkey
                    </button>
                    <p style={{ fontSize: 11, color: '#555', marginTop: 24 }}>TEE Protected • zk-SNARKs • No password stored</p>
                </div>
            </div>
        );
    }
    const [step, setStep] = useState<TransferStep>(1);
    const [fromCur, setFromCur] = useState(CURRENCIES[0]);
    const [toCur, setToCur] = useState(CURRENCIES[2]);
    const [amount, setAmount] = useState(0);
    const [recipient, setRecipient] = useState('');
    const [method, setMethod] = useState<'bank' | 'mobile' | 'wallet'>('bank');
    const rate = fromCur && toCur && amount > 0 ? (1 / 30.9) * 0.985 : 0;
    const fee = amount * 0.001;
    const received = amount * rate;

    const handleConfirm = () => { alert('✅ Transfer sent via Smart Account — settled in <5s'); setStep(1); setAmount(0); };

    return (
        <div style={{ fontFamily: 'Arial', background: '#0a0a1a', minHeight: '100vh', color: '#fff', direction: 'rtl' }}>
            <nav style={{ background: '#141428', padding: '16px 24px', display: 'flex', justifyContent: 'space-between', alignItems: 'center', borderBottom: '1px solid #2a2a4a' }}>
                <h1 style={{ color: '#00d4aa', margin: 0, fontSize: 22 }}>SwiftBridge</h1>
                <div style={{ display: 'flex', gap: 8 }}>
                    {['transfer', 'dashboard', 'dao'].map(p => (
                        <button key={p} onClick={() => setPage(p as any)}
                            style={{ padding: '8px 18px', background: page === p ? '#00d4aa' : '#1a1a3a', color: '#fff', border: 'none', borderRadius: 6, cursor: 'pointer', fontSize: 13 }}>
                            {p === 'transfer' ? 'تحويل' : p === 'dashboard' ? 'الرئيسية' : 'DAO'}
                        </button>
                    ))}
                </div>
            </nav>

            <div style={{ maxWidth: 700, margin: '20px auto', padding: '0 16px' }}>
                {page === 'transfer' && (
                    <div style={{ background: '#141428', borderRadius: 12, padding: 24 }}>
                        <h2 style={{ margin: 0 }}>تحويل <span style={{ color: '#00d4aa', fontSize: 13 }}>Chain Abstraction — لا تعقيدات</span></h2>
                        <div style={{ display: 'flex', gap: 4, justifyContent: 'center', margin: '16px 0' }}>
                            {[1, 2, 3].map(s => (
                                <div key={s} style={{ display: 'flex', alignItems: 'center' }}>
                                    <div style={{ width: 30, height: 30, borderRadius: 15, background: step >= s ? '#00d4aa' : '#2a2a4a', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 13 }}>{s}</div>
                                    {s < 3 && <div style={{ width: 50, height: 2, background: step > s ? '#00d4aa' : '#2a2a4a', margin: '0 4px' }} />}
                                </div>
                            ))}
                        </div>

                        {step === 1 && <>
                            <h3>الخطوة 1: اختر العملات والمبلغ</h3>
                            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
                                <div><label style={{ color: '#888', fontSize: 12 }}>من</label>
                                    <select value={fromCur.code} onChange={e => setFromCur(CURRENCIES.find(c => c.code === e.target.value) || fromCur)}
                                        style={{ width: '100%', padding: 10, background: '#0a0a1a', color: '#fff', border: '1px solid #2a2a4a', borderRadius: 6, marginTop: 4 }}>
                                        {CURRENCIES.map(c => <option key={c.code} value={c.code}>{c.flag} {c.code}</option>)}
                                    </select></div>
                                <div><label style={{ color: '#888', fontSize: 12 }}>إلى</label>
                                    <select value={toCur.code} onChange={e => setToCur(CURRENCIES.find(c => c.code === e.target.value) || toCur)}
                                        style={{ width: '100%', padding: 10, background: '#0a0a1a', color: '#fff', border: '1px solid #2a2a4a', borderRadius: 6, marginTop: 4 }}>
                                        {CURRENCIES.map(c => <option key={c.code} value={c.code}>{c.flag} {c.code}</option>)}
                                    </select></div>
                            </div>
                            <div style={{ marginTop: 12 }}>
                                <label style={{ color: '#888', fontSize: 12 }}>المبلغ</label>
                                <input type="number" value={amount || ''} onChange={e => setAmount(parseFloat(e.target.value) || 0)}
                                    placeholder="0.00"
                                    style={{ width: '100%', padding: 10, background: '#0a0a1a', color: '#fff', border: '1px solid #2a2a4a', borderRadius: 6, marginTop: 4, fontSize: 18 }} />
                            </div>
                            {amount > 0 && <div style={{ background: '#0a0a1a', borderRadius: 8, padding: 12, marginTop: 12 }}>
                                <p style={{ margin: '2px 0' }}>💰 السعر: 1 {fromCur.code} = {rate.toFixed(6)} {toCur.code}</p>
                                <p style={{ margin: '2px 0' }}>📊 الرسوم: ${fee.toFixed(2)} (0.1%)</p>
                                <p style={{ margin: '2px 0', color: '#00d4aa' }}>✅ سيستلم: {received.toFixed(2)} {toCur.code}</p>
                            </div>}
                        </>}

                        {step === 2 && <>
                            <h3>الخطوة 2: بيانات المستلم</h3>
                            <div style={{ display: 'flex', gap: 8, marginBottom: 12 }}>
                                {(['bank', 'mobile', 'wallet'] as const).map(m => (
                                    <button key={m} onClick={() => setMethod(m)}
                                        style={{ flex: 1, padding: 10, background: method === m ? '#00d4aa' : '#0a0a1a', color: '#fff', border: '1px solid #2a2a4a', borderRadius: 6, cursor: 'pointer' }}>
                                        {m === 'bank' ? '🏦 بنك' : m === 'mobile' ? '📱 محمول' : '👛 محفظة'}
                                    </button>
                                ))}
                            </div>
                            <input type="text" value={recipient} onChange={e => setRecipient(e.target.value)} placeholder="اسم المستلم"
                                style={{ width: '100%', padding: 10, background: '#0a0a1a', color: '#fff', border: '1px solid #2a2a4a', borderRadius: 6, marginBottom: 8 }} />
                            <input type="text" placeholder={method === 'bank' ? 'IBAN' : method === 'mobile' ? 'رقم المحمول' : 'معرف المحفظة'}
                                style={{ width: '100%', padding: 10, background: '#0a0a1a', color: '#fff', border: '1px solid #2a2a4a', borderRadius: 6 }} />
                        </>}

                        {step === 3 && <>
                            <h3>الخطوة 3: تأكيد</h3>
                            <div style={{ background: '#0a0a1a', borderRadius: 8, padding: 16 }}>
                                {[{ l: 'من', v: `${fromCur.flag} ${fromCur.code}` }, { l: 'إلى', v: `${toCur.flag} ${toCur.code}` },
                                  { l: 'المبلغ', v: `$${amount.toFixed(2)}` }, { l: 'سيستلم', v: `${received.toFixed(2)} ${toCur.code}` },
                                  { l: 'المستلم', v: recipient }].map((item, i) => (
                                    <div key={i} style={{ display: 'flex', justifyContent: 'space-between', padding: '6px 0', borderBottom: i < 4 ? '1px solid #2a2a4a' : 'none' }}>
                                        <span style={{ color: '#888' }}>{item.l}</span><span style={{ fontWeight: 'bold' }}>{item.v}</span>
                                    </div>
                                ))}
                            </div>
                            <p style={{ color: '#00d4aa', fontSize: 12, marginTop: 8 }}>🛡️ محمي بـ zk-SNARKs • TEE • DOT settlement</p>
                        </>}

                        <div style={{ display: 'flex', gap: 8, marginTop: 16 }}>
                            {step > 1 && <button onClick={() => setStep(p => (p - 1) as TransferStep)}
                                style={{ flex: 1, padding: 12, background: '#2a2a4a', color: '#fff', border: 'none', borderRadius: 6, cursor: 'pointer' }}>رجوع</button>}
                            <button onClick={step === 3 ? handleConfirm : () => setStep(p => (p + 1) as TransferStep)}
                                style={{ flex: 2, padding: 12, background: '#00d4aa', color: '#000', border: 'none', borderRadius: 6, cursor: 'pointer', fontWeight: 'bold' }}>
                                {step === 3 ? '✅ تأكيد عبر Smart Account' : 'التالي'}
                            </button>
                        </div>
                    </div>
                )}

                {page === 'dashboard' && <div>
                    <h2>الرئيسية</h2>
                    <div style={{ background: '#141428', borderRadius: 10, padding: 16, marginBottom: 12, border: '1px solid #00d4aa' }}>
                        <h3 style={{ margin: '0 0 8px', color: '#00d4aa', fontSize: 14 }}>🔐 Smart Account</h3>
                        <p style={{ color: '#888', fontSize: 12, wordBreak: 'break-all' }}>📍 {smartAccount.address}</p>
                        <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
                            <span style={{ background: '#00d4aa22', color: '#00d4aa', padding: '2px 10px', borderRadius: 4, fontSize: 11 }}>✅ Biometric</span>
                            <span style={{ background: '#00d4aa22', color: '#00d4aa', padding: '2px 10px', borderRadius: 4, fontSize: 11 }}>✅ Passkey</span>
                            <span style={{ background: '#00d4aa22', color: '#00d4aa', padding: '2px 10px', borderRadius: 4, fontSize: 11 }}>🔐 TSS</span>
                            <span style={{ background: '#646cff22', color: '#646cff', padding: '2px 10px', borderRadius: 4, fontSize: 11 }}>🛡️ TEE</span>
                            <span style={{ background: '#646cff22', color: '#646cff', padding: '2px 10px', borderRadius: 4, fontSize: 11 }}>🌀 zk</span>
                        </div>
                    </div>
                    <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 12, marginBottom: 16 }}>
                        {[{ c: 'USD', bal: 12580 }, { c: 'EUR', bal: 3200 }, { c: 'EGP', bal: 150000 }].map(({ c, bal }) => (
                            <div key={c} style={{ background: '#141428', borderRadius: 10, padding: 16, textAlign: 'center' }}>
                                <div style={{ fontSize: 24, marginBottom: 4, opacity: 0.7 }}>{c}</div>
                                <div style={{ fontSize: 20, fontWeight: 'bold', color: '#00d4aa' }}>{bal.toLocaleString()}</div>
                            </div>
                        ))}
                    </div>
                    <div style={{ background: '#141428', borderRadius: 10, padding: 16 }}>
                        <p style={{ color: '#888', fontSize: 12 }}>🛡️ Biometric + Passkey • Chain Abstraction • No gas fees • TEE • zk-SNARKs • Unilateral Recovery</p>
                    </div>
                </div>}

                {page === 'dao' && <div>
                    <h2>DAO Governance</h2>
                    <div style={{ background: '#141428', borderRadius: 10, padding: 16, marginBottom: 12 }}>
                        <h4 style={{ margin: '0 0 8px', color: '#00d4aa' }}>🗳️ استفتاء: تعديل رسوم التحويل</h4>
                        <p style={{ fontSize: 13, color: '#aaa' }}>خفض رسوم التحويلات فوق $50K من 0.1% إلى 0.05%</p>
                        <div style={{ display: 'flex', gap: 12, marginTop: 8 }}>
                            <button style={{ padding: '8px 24px', background: '#00d4aa', color: '#000', border: 'none', borderRadius: 6, cursor: 'pointer', fontWeight: 'bold' }}>✅ تأييد</button>
                            <button style={{ padding: '8px 24px', background: '#e94560', color: '#fff', border: 'none', borderRadius: 6, cursor: 'pointer' }}>❌ رفض</button>
                        </div>
                    </div>
                    <div style={{ background: '#141428', borderRadius: 10, padding: 16 }}>
                        <p style={{ fontSize: 12, color: '#888' }}>🔐 DAO • Code is Law • No Human Control • Self-Sovereign</p>
                    </div>
                </div>}
            </div>
        </div>
    );
};

export default App;
