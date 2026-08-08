import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { getSession } from '../stores/auth';
import LoginView from '../views/LoginView';
import DashboardView from '../views/DashboardView';

function RequireAuth({ children }: { children: JSX.Element }) {
  const session = getSession();
  if (!session) {
    return <Navigate to="/login" replace />;
  }
  return children;
}

export default function AppRouter() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/login" element={<LoginView />} />
        <Route
          path="/"
          element={
            <RequireAuth>
              <DashboardView />
            </RequireAuth>
          }
        />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </BrowserRouter>
  );
}
