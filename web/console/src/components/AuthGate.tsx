import { KeyRound, LoaderCircle, ShieldCheck } from 'lucide-react';
import { useState, type FormEvent } from 'react';

import { displayError } from '../utils/errors';
import './AuthGate.css';

export function AuthGate({
  checking = false,
  onAuthenticate,
}: {
  checking?: boolean;
  onAuthenticate?: (token: string) => Promise<void> | void;
}) {
  const [token, setToken] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string>();

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!token.trim() || submitting) return;

    setSubmitting(true);
    setError(undefined);
    try {
      await onAuthenticate?.(token.trim());
    } catch (submitError) {
      setError(displayError(submitError));
    } finally {
      setSubmitting(false);
    }
  };

  if (checking) {
    return (
      <main className="auth-screen" aria-busy="true">
        <section className="auth-panel auth-checking" aria-label="正在验证控制台会话">
          <LoaderCircle className="auth-spinner" size={22} aria-hidden="true" />
          <strong>正在验证控制台会话</strong>
        </section>
      </main>
    );
  }

  return (
    <main className="auth-screen">
      <section className="auth-panel" aria-labelledby="auth-title">
        <header className="auth-header">
          <span className="auth-mark" aria-hidden="true"><ShieldCheck size={22} /></span>
          <div>
            <span className="auth-kicker">EDGE AGENT CONTROL CENTER</span>
            <h1 id="auth-title">管理控制台认证</h1>
          </div>
        </header>

        <form className="auth-form" onSubmit={handleSubmit}>
          <label htmlFor="api-token">访问令牌</label>
          <div className="auth-token-field">
            <KeyRound size={17} aria-hidden="true" />
            <input
              autoComplete="current-password"
              autoFocus
              id="api-token"
              onChange={(event) => setToken(event.target.value)}
              placeholder="输入控制台访问令牌"
              type="password"
              value={token}
            />
          </div>
          {error ? <p className="auth-error" role="alert">{error}</p> : null}
          <button className="auth-submit" disabled={!token.trim() || submitting} type="submit">
            {submitting ? <LoaderCircle className="auth-spinner" size={17} aria-hidden="true" /> : <ShieldCheck size={17} aria-hidden="true" />}
            {submitting ? '正在验证' : '进入控制台'}
          </button>
        </form>
      </section>
    </main>
  );
}
