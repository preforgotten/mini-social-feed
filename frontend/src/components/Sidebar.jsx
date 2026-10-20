import { A, useLocation, useNavigate } from '@solidjs/router';
import { Show } from 'solid-js';
import { session, logout } from '../session';

export default function Sidebar() {
  const location = useLocation();
  const navigate = useNavigate();
  const s = () => session() || {};
  const initial = () => (s().username || '?')[0].toUpperCase();
  const pathname = () => location.pathname;

  function isActive(path) {
    if (path === '/') return pathname() === '/';
    return pathname().startsWith(path);
  }

  function handleLogout() {
    logout();
    navigate('/login');
  }

  return (
    <aside class="sidebar">
      <div class="sidebar-brand">
        <div class="brand-icon">▸</div>
        <span>mini-feed</span>
      </div>

      <nav class="sidebar-nav">
        <A href="/" classList={{ active: isActive('/') }}>
          <span class="nav-icon">⌂</span>
          <span>Feed</span>
        </A>
        <A href={`/profile/${s().userId}`} classList={{ active: isActive('/profile') }}>
          <span class="nav-icon">◉</span>
          <span>Profile</span>
        </A>
        <A href="/media" classList={{ active: isActive('/media') }}>
          <span class="nav-icon">◫</span>
          <span>Media</span>
        </A>
      </nav>

      <div class="sidebar-user">
        <A href={`/profile/${s().userId}`} class="user-avatar">
          <Show when={s().avatarUrl} fallback={initial()}>
            <img src={s().avatarUrl} alt="" />
          </Show>
        </A>
        <div class="user-info">
          <A href={`/profile/${s().userId}`} class="user-name">{s().username}</A>
          <div class="user-handle">@{s().username}</div>
        </div>
        <button class="btn-logout" onClick={handleLogout} title="Log out">⏻</button>
      </div>
    </aside>
  );
}
