import { render } from 'solid-js/web';
import { Router, Route } from '@solidjs/router';
import App from './App';
import Feed from './pages/Feed';
import Login from './pages/Login';
import Register from './pages/Register';
import Profile from './pages/Profile';
import Thread from './pages/Thread';
import Media from './pages/Media';

render(
  () => (
    <Router root={App}>
      <Route path="/" component={Feed} />
      <Route path="/login" component={Login} />
      <Route path="/register" component={Register} />
      <Route path="/profile/:userId" component={Profile} />
      <Route path="/thread/:postId" component={Thread} />
      <Route path="/media" component={Media} />
    </Router>
  ),
  document.getElementById('root'),
);
