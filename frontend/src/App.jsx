import { Show } from 'solid-js';
import { useLocation, useNavigate } from '@solidjs/router';
import { session } from './session';
import Sidebar from './components/Sidebar';

export default function App(props) {
  const location = useLocation();

  const isAuthPage = () => {
    const p = location.pathname;
    return p === '/login' || p === '/register';
  };

  const showSidebar = () => !!session() && !isAuthPage();

  return (
    <div id="app">
      <Show when={showSidebar()}>
        <Sidebar />
      </Show>
      <main classList={{ 'full-width': !showSidebar() }}>
        {props.children}
      </main>
    </div>
  );
}
