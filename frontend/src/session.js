import { createSignal } from 'solid-js';

function loadSession() {
  try {
    const raw = localStorage.getItem('session');
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

const [session, setSession] = createSignal(loadSession());

/** Centralized persistence — the only place that writes to localStorage. */
function persist(s) {
  if (s) {
    localStorage.setItem('token', s.token);
    localStorage.setItem('session', JSON.stringify(s));
  } else {
    localStorage.removeItem('token');
    localStorage.removeItem('session');
  }
  setSession(s);
}

// --- cross-tab sync --------------------------------------------------------

window.addEventListener('storage', (e) => {
  if (e.key === 'session' || e.key === 'token') {
    // another tab wrote to localStorage — reload into the signal
    if (e.key === 'session' && e.newValue === null) {
      setSession(null);
    } else if (e.key === 'token' && e.newValue === null) {
      setSession(null);
    } else if (e.key === 'session') {
      try {
        setSession(JSON.parse(e.newValue));
      } catch {
        setSession(null);
      }
    }
  }
});

// --- public API ------------------------------------------------------------

export { session, setSession };

/** Called after successful login or registration. */
export function login(token, userId, username, avatarUrl) {
  persist({ token, userId, username, avatarUrl });
}

/** Clear session from the signal. Caller should navigate away. */
export function logout() {
  persist(null);
}

/**
 * Merge partial fields into the current session object.
 * Nothing happens if there is no active session.
 */
export function updateSession(partial) {
  const cur = session();
  if (!cur) return;
  persist({ ...cur, ...partial });
}
