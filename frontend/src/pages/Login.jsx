import { createSignal, Show } from 'solid-js';
import { A, useNavigate } from '@solidjs/router';
import { api } from '../api';
import { login, session } from '../session';

export default function LoginPage() {
  const navigate = useNavigate();
  const [username, setUsername] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [error, setError] = createSignal('');
  const [loading, setLoading] = createSignal(false);

  // Redirect if already logged in
  if (session()) {
    navigate('/', { replace: true });
  }

  async function handleSubmit(e) {
    e.preventDefault();
    if (!username().trim()) {
      setError('Username is required');
      return;
    }
    if (!password()) {
      setError('Password is required');
      return;
    }
    setLoading(true);
    setError('');
    try {
      const data = await api('/auth/login', {
        method: 'POST',
        body: JSON.stringify({ username: username().trim(), password: password() }),
      });
      login(data.access_token, data.user.id, data.user.username, data.user.avatar_url);
      navigate('/', { replace: true });
    } catch (err) {
      setError(err.message);
    }
    setLoading(false);
  }

  return (
    <div class="auth-page">
      <div class="auth-brand">mini-feed</div>
      <h1>Sign in</h1>
      <p class="auth-subtitle">Enter your credentials to continue.</p>

      <form onSubmit={handleSubmit}>
        <div class="form-group">
          <label>Username</label>
          <input
            value={username()}
            onInput={e => setUsername(e.target.value)}
            maxLength={64}
            placeholder="yourname"
            autofocus
          />
        </div>
        <div class="form-group">
          <label>Password</label>
          <input
            type="password"
            value={password()}
            onInput={e => setPassword(e.target.value)}
            maxLength={128}
            placeholder="Your password"
          />
        </div>
        <Show when={error()}>
          <div class="form-error">{error()}</div>
        </Show>
        <button type="submit" class="btn-primary" disabled={loading()}>
          {loading() ? 'Signing in…' : 'Sign in'}
        </button>
      </form>

      <p class="form-footer">
        Don't have an account? <A href="/register">Create one</A>
      </p>
    </div>
  );
}
