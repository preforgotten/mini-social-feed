import { createSignal, Show } from 'solid-js';
import { A, useNavigate } from '@solidjs/router';
import { api } from '../api';
import { login, session } from '../session';

export default function RegisterPage() {
  const navigate = useNavigate();
  const [username, setUsername] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [bio, setBio] = createSignal('');
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
    if (!password() || password().length < 4) {
      setError('Password must be at least 4 characters');
      return;
    }
    setLoading(true);
    setError('');
    try {
      const data = await api('/users/register', {
        method: 'POST',
        body: JSON.stringify({
          username: username().trim(),
          password: password(),
          bio: bio().trim() || undefined,
        }),
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
      <h1>Create account</h1>
      <p class="auth-subtitle">Set up your profile and get started.</p>

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
            placeholder="Minimum 4 characters"
          />
        </div>
        <div class="form-group">
          <label>Bio (optional)</label>
          <textarea
            value={bio()}
            onInput={e => setBio(e.target.value)}
            maxLength={512}
            rows={2}
            placeholder="A short description about yourself…"
          />
        </div>
        <Show when={error()}>
          <div class="form-error">{error()}</div>
        </Show>
        <button type="submit" class="btn-primary" disabled={loading()}>
          {loading() ? 'Creating account…' : 'Create account'}
        </button>
      </form>

      <p class="form-footer">
        Already have an account? <A href="/login">Sign in</A>
      </p>
    </div>
  );
}
